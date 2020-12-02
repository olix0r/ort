use crate::{latency, Distribution, Error, Target};
use futures::{prelude::*, stream::FuturesUnordered};
use ort_core::{limit::Acquire, MakeOrt, Ort, Spec};
use rand::{distributions::Distribution as _, thread_rng};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tracing::{debug, debug_span, info, trace};
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct Runner<L> {
    clients: usize,
    limit: L,
    countdown: Arc<Countdown>,
    response_latencies: Arc<latency::Distribution>,
    response_sizes: Arc<Distribution>,
}

#[derive(Debug)]
struct Countdown {
    total: usize,
    count: AtomicUsize,
}

impl Default for Countdown {
    fn default() -> Self {
        Self::from(std::usize::MAX)
    }
}

impl From<usize> for Countdown {
    fn from(total: usize) -> Self {
        Self {
            total,
            count: 0.into(),
        }
    }
}

impl Countdown {
    fn next(&self) -> Option<usize> {
        let total = self.total;
        self.count
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, move |c| {
                if c == total {
                    None
                } else {
                    Some(c + 1)
                }
            })
            .ok()
    }
}

impl<L: Acquire> Runner<L> {
    pub fn new(
        clients: usize,
        total_requests: Option<usize>,
        limit: L,
        response_latencies: Arc<latency::Distribution>,
        response_sizes: Arc<Distribution>,
    ) -> Self {
        let total_requests = total_requests.filter(|n| *n > 0).unwrap_or(std::usize::MAX);
        Self {
            clients,
            countdown: Arc::new(total_requests.into()),
            limit,
            response_latencies,
            response_sizes,
        }
    }

    pub async fn run<C>(self, connect: C, target: Target) -> Result<(), Error>
    where
        C: MakeOrt<Target>,
    {
        let mut tasks = FuturesUnordered::new();

        for c in 0..self.clients {
            let Self {
                clients: _,
                limit,
                countdown,
                response_latencies,
                response_sizes,
            } = self.clone();
            let mut connect = connect.clone();
            let target = target.clone();

            debug!(c, ?countdown, %target, "Initializing new client");
            let client = tokio::spawn(
                async move {
                    let client = connect.make_ort(target).await?;
                    while let Some(n) = countdown.next() {
                        let spec = {
                            let mut rng = thread_rng();
                            Spec {
                                latency: response_latencies.sample(&mut rng),
                                response_size: response_sizes.sample(&mut rng) as usize,
                            }
                        };

                        let permit = limit.acquire().await;
                        let mut client = client.clone();
                        tokio::spawn(
                            async move {
                                trace!(?spec, "Sending request");
                                match client.ort(spec).await {
                                    Ok(_) => trace!(n, "Request complete"),
                                    Err(error) => info!(%error, n, "Request failed"),
                                }
                                drop(permit);
                            }
                            .in_current_span(),
                        );
                    }

                    Ok::<_, Error>(())
                }
                .instrument(debug_span!("client", c)),
            );
            tasks.push(client);
        }

        while let Some(_) = tasks.next().await {}

        Ok(())
    }
}
