use crate::{latency, Distribution, Error, Target};
use futures::{prelude::*, stream::FuturesUnordered};
use ort_core::{limit::Acquire, MakeOrt, Ort, Spec};
use rand::{distributions::Distribution as _, thread_rng};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tracing::{debug, debug_span, info, trace, Instrument};

#[derive(Clone)]
pub struct Runner<L> {
    clients: usize,
    limit: L,
    counter: Arc<Counter>,
    response_latencies: Arc<latency::Distribution>,
    response_sizes: Arc<Distribution>,
}

#[derive(Debug)]
struct Counter {
    limit: Option<usize>,
    count: AtomicUsize,
}

// === impl Runner ===

impl<L: Acquire> Runner<L> {
    pub fn new(
        clients: usize,
        total_requests: Option<usize>,
        limit: L,
        response_latencies: Arc<latency::Distribution>,
        response_sizes: Arc<Distribution>,
    ) -> Self {
        Self {
            clients,
            counter: Arc::new(Counter::from(total_requests)),
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
                counter,
                response_latencies,
                response_sizes,
            } = self.clone();
            let mut connect = connect.clone();
            let target = target.clone();

            debug!(c, %target, "Initializing new client");
            let client = tokio::spawn(
                async move {
                    let client = connect.make_ort(target).await?;

                    while let Some(n) = counter.next() {
                        let permit = limit.acquire().await;

                        let spec = {
                            let mut rng = thread_rng();
                            Spec {
                                latency: response_latencies.sample(&mut rng),
                                response_size: response_sizes.sample(&mut rng) as usize,
                            }
                        };

                        let mut client = client.clone();
                        tokio::spawn(
                            async move {
                                trace!(?spec, "Sending request");
                                match client.ort(spec).await {
                                    Ok(_) => trace!("Request complete"),
                                    Err(error) => info!(%error, "Request failed"),
                                }
                                drop(permit);
                            }
                            .instrument(debug_span!("request", n)),
                        );
                    }

                    Ok::<_, Error>(())
                }
                .instrument(debug_span!("client", c)),
            );
            tasks.push(client);
        }

        while tasks.next().await.is_some() {}

        Ok(())
    }
}

// === impl Counter ===

impl Default for Counter {
    fn default() -> Self {
        Self::from(None)
    }
}

impl From<Option<usize>> for Counter {
    fn from(limit: Option<usize>) -> Self {
        Self {
            limit: limit.filter(|l| *l > 0),
            count: 0.into(),
        }
    }
}

impl Counter {
    fn next(&self) -> Option<usize> {
        let limit = self.limit;
        self.count
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, move |c| {
                if Some(c) == limit {
                    None
                } else {
                    Some(c + 1)
                }
            })
            .ok()
    }
}
