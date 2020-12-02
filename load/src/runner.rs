use crate::{latency, limit::Acquire, Distribution, Error, Target};
use ort_core::{MakeOrt, Ort, Spec};
use rand::{distributions::Distribution as _, thread_rng};
use std::sync::Arc;
use tracing::{debug, info, trace};
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct Runner<L> {
    total_requests: usize,
    requests_per_client: Option<usize>,
    limit: L,
    response_latencies: Arc<latency::Distribution>,
    response_sizes: Arc<Distribution>,
}

impl<L: Acquire> Runner<L> {
    pub fn new(
        total_requests: Option<usize>,
        requests_per_client: Option<usize>,
        limit: L,
        response_latencies: Arc<latency::Distribution>,
        response_sizes: Arc<Distribution>,
    ) -> Self {
        Self {
            total_requests: total_requests.unwrap_or(std::usize::MAX),
            requests_per_client: requests_per_client.filter(|n| *n > 0),
            limit,
            response_latencies,
            response_sizes,
        }
    }

    pub async fn run<C>(self, mut connect: C, target: Target) -> Result<(), Error>
    where
        C: MakeOrt<Target> + Clone + Send + 'static,
        C::Ort: Clone + Send + 'static,
    {
        let Self {
            limit,
            requests_per_client,
            total_requests,
            response_latencies,
            response_sizes,
        } = self;

        debug!(total_requests, ?requests_per_client, %target, "Initializing new client");
        let mut client = connect.make_ort(target.clone()).await?;

        for n in 0..total_requests {
            if let Some(rpc) = requests_per_client {
                if n % rpc == 0 {
                    debug!("Creating new client");
                    client = connect.make_ort(target.clone()).await?;
                }
            }
            let mut client = client.clone();

            let spec = {
                let mut rng = thread_rng();
                Spec {
                    latency: response_latencies.sample(&mut rng),
                    response_size: response_sizes.sample(&mut rng) as usize,
                    ..Default::default()
                }
            };

            let permit = limit.acquire().await;
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

        Ok(())
    }
}
