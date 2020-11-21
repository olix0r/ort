use crate::{latency, limit::Acquire, Distribution, Target};
use ort_core::{MakeOrt, Ort, Spec};
use rand::{distributions::Distribution as _, rngs::SmallRng};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tracing::{debug, debug_span, info, trace};
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct Runner<L> {
    clients_per_target: usize,
    requests_per_target: usize,
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
        clients_per_target: usize,
        requests_per_target: usize,
        total_requests: usize,
        limit: L,
        response_latencies: Arc<latency::Distribution>,
        response_sizes: Arc<Distribution>,
        rng: SmallRng,
    ) -> Self {
        let total_requests = Countdown::new(total_requests);
        Self {
            clients_per_target,
            requests_per_target,
            limit,
            total_requests,
            response_latencies,
            response_sizes,
            rng,
        }
    }

    pub async fn run<C>(self, mut connect: C, target: Target)
    where
        C: MakeOrt<Target> + Clone + Send + 'static,
        C::Ort: Clone + Send + 'static,
    {
        let Self {
            clients_per_target,
            requests_per_target,
            limit,
            total_requests,
            response_latencies,
            response_sizes,
            rng,
        } = self;

        let requests_per_target = Countdown::new(requests_per_target);
        let mut handles = Vec::with_capacity(clients_per_target);
        for _ in 0..clients_per_target {
            let client = match connect.make_ort(target.clone()).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let limit = limit.clone();
            let requests_per_target = requests_per_target.clone();
            let response_latencies = response_latencies.clone();
            let response_sizes = response_sizes.clone();
            let total_requests = total_requests.clone();
            let mut rng = rng.clone();
            let h = tokio::spawn(
                async move {
                    debug!(?requests_per_target.limit, ?total_requests.limit, "Sending requests");
                    loop {
                        let r = match requests_per_target.advance() {
                            Ok(r) => r,
                            Err(()) => {
                                debug!("No more requests to this target");
                                return;
                            }
                        };
                        let n = match total_requests.advance() {
                            Ok(n) => n,
                            Err(()) => {
                                debug!("No more requests to any target");
                                return;
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
                                    latency: latency.into(),
                                    response_size: response_size as usize,
                                    ..Default::default()
                                };
                                trace!(%response_size, request = %r, "Sending request");
                                match client.ort(spec).await {
                                    Ok(rsp) => {
                                        trace!(r, n, rsp_sz = rsp.data.len(), "Request complete")
                                    }
                                    Err(error) => info!(%error, r, n, "Request failed"),
                                }
                                drop(permit);
                            }
                            .in_current_span(),
                        );
                    }
                }
                .instrument(debug_span!("client", %target)),
            );
            handles.push(h);
        }

        debug!(%target, "Complete");
    }
}
