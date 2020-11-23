use crate::{latency, limit::Acquire, Distribution, Error, Target};
use ort_core::{MakeOrt, Ort, Spec};
use rand::{distributions::Distribution as _, rngs::SmallRng};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tracing::{debug, info, trace};
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct Runner<L> {
    limit: L,
    total_requests: Countdown,
    response_latencies: Arc<latency::Distribution>,
    response_sizes: Arc<Distribution>,
    rng: SmallRng,
}

#[derive(Clone)]
struct Countdown {
    limit: Option<usize>,
    issued: Arc<AtomicUsize>,
}

impl Countdown {
    fn new(limit: usize) -> Self {
        Self {
            limit: if limit == 0 { None } else { Some(limit) },
            issued: Arc::new(0.into()),
        }
    }

    fn advance(&self) -> Result<usize, ()> {
        let n = self.issued.fetch_add(1, Ordering::SeqCst);
        if let Some(limit) = self.limit {
            trace!(n, limit);
            if n >= limit {
                return Err(());
            }
        }
        Ok(n)
    }
}

impl<L: Acquire> Runner<L> {
    pub fn new(
        total_requests: usize,
        limit: L,
        response_latencies: Arc<latency::Distribution>,
        response_sizes: Arc<Distribution>,
        rng: SmallRng,
    ) -> Self {
        let total_requests = Countdown::new(total_requests);
        Self {
            limit,
            total_requests,
            response_latencies,
            response_sizes,
            rng,
        }
    }

    pub async fn run<C>(self, mut connect: C, target: Target) -> Result<(), Error>
    where
        C: MakeOrt<Target> + Clone + Send + 'static,
        C::Ort: Clone + Send + 'static,
    {
        let Self {
            limit,
            total_requests,
            response_latencies,
            response_sizes,
            mut rng,
        } = self;

        debug!(?total_requests.limit, %target, "Initializing new client");
        let client = connect.make_ort(target.clone()).await?;
        loop {
            let n = match total_requests.advance() {
                Ok(n) => n,
                Err(()) => {
                    debug!("No more requests to any target");
                    return Ok(());
                }
            };
            let permit = limit.acquire().await;
            trace!("Acquired permit");

            let latency: Duration = response_latencies.sample(&mut rng);
            let response_size = response_sizes.sample(&mut rng);
            let mut client = client.clone();
            tokio::spawn(
                async move {
                    let spec = Spec {
                        latency,
                        response_size: response_size as usize,
                        ..Default::default()
                    };
                    trace!(%response_size, ?latency, "Sending request");
                    match client.ort(spec).await {
                        Ok(_) => trace!(n, "Request complete"),
                        Err(error) => info!(%error, n, "Request failed"),
                    }
                    drop(permit);
                }
                .in_current_span(),
            );
        }
    }
}
