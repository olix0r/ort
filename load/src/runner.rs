use crate::{proto, rate_limit::Acquire, Client, Distribution, MakeClient};
use rand::{rngs::SmallRng, Rng};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Semaphore;
use tracing::{debug_span, trace};
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct Runner {
    clients: usize,
    concurrency: Option<Arc<Semaphore>>,
    rate_limit: Acquire,
    total_requests: Option<TotalRequestsLimit>,
    response_sizes: Arc<Distribution>,
    rng: SmallRng,
}

#[derive(Clone)]
struct TotalRequestsLimit {
    limit: usize,
    issued: Arc<AtomicUsize>,
}

impl Runner {
    pub fn new(
        clients: usize,
        concurrency: Option<Arc<Semaphore>>,
        total_requests: usize,
        rate_limit: Acquire,
        response_sizes: Arc<Distribution>,
        rng: SmallRng,
    ) -> Self {
        assert!(clients > 0);
        let total_requests = if total_requests == 0 {
            None
        } else {
            Some(TotalRequestsLimit {
                limit: total_requests,
                issued: Arc::new(0.into()),
            })
        };
        Self {
            clients,
            concurrency,
            rate_limit,
            total_requests,
            response_sizes,
            rng,
        }
    }

    pub async fn run<C>(self, mut connect: C)
    where
        C: MakeClient + Clone + Send + 'static,
        C::Client: Clone + Send + 'static,
    {
        let Self {
            clients,
            concurrency,
            rate_limit,
            total_requests,
            response_sizes,
            rng,
        } = self;

        for c in 0..clients {
            let client = connect.make_client().await;

            let total_requests = total_requests.clone();
            let concurrency = concurrency.clone();
            let limit = rate_limit.clone();
            let rsp_sizes = response_sizes.clone();
            let mut rng = rng.clone();
            let client = client.clone();
            tokio::spawn(
                async move {
                    loop {
                        if let Some(lim) = total_requests.as_ref() {
                            if lim.issued.fetch_add(1, Ordering::Release) >= lim.limit {
                                return;
                            }
                        }
                        let permits = if let Some(c) = concurrency.clone() {
                            let (l, c) = tokio::join!(limit.acquire(), c.acquire_owned());
                            (l, Some(c))
                        } else {
                            (limit.acquire().await, None)
                        };
                        trace!("Acquired permits");

                        // TODO generate request params (latency, error).

                        let rsp_sz = rsp_sizes.get(rng.gen());

                        let spec = proto::ResponseSpec {
                            result: Some(proto::response_spec::Result::Success(
                                proto::response_spec::Success {
                                    size: rsp_sz as i64,
                                },
                            )),
                            ..Default::default()
                        };

                        let mut client = client.clone();
                        tokio::spawn(async move {
                            let _ = client.get(spec).await;
                            drop(permits);
                        });
                    }
                }
                .instrument(debug_span!("runner", c)),
            );
        }
    }
}
