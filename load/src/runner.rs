use crate::{limit::Acquire, proto, Client, Distribution, MakeClient};
use rand::{distributions::Distribution as _, rngs::SmallRng};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tracing::{debug_span, trace};
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct Runner<L> {
    clients: usize,
    streams: usize,
    limit: L,
    total_requests: Option<TotalRequestsLimit>,
    response_sizes: Arc<Distribution>,
    rng: SmallRng,
}

#[derive(Clone)]
struct TotalRequestsLimit {
    limit: usize,
    issued: Arc<AtomicUsize>,
}

impl<L: Acquire> Runner<L> {
    pub fn new(
        clients: usize,
        streams: usize,
        total_requests: usize,
        limit: L,
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
            streams,
            limit,
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
            streams,
            limit,
            total_requests,
            response_sizes,
            rng,
        } = self;

        for c in 0..clients {
            let client = connect.make_client().await;

            for s in 0..streams {
                let total_requests = total_requests.clone();
                let limit = limit.clone();
                let rsp_sizes = response_sizes.clone();
                let mut rng = rng.clone();
                let mut client = client.clone();
                tokio::spawn(
                    async move {
                        loop {
                            if let Some(lim) = total_requests.as_ref() {
                                if lim.issued.fetch_add(1, Ordering::Release) >= lim.limit {
                                    return;
                                }
                            }
                            let permit = limit.acquire().await;
                            trace!("Acquired permit");

                            // TODO generate request params (latency, error).

                            let rsp_sz = rsp_sizes.sample(&mut rng);

                            let spec = proto::ResponseSpec {
                                result: Some(proto::response_spec::Result::Success(
                                    proto::response_spec::Success {
                                        size: rsp_sz as i64,
                                    },
                                )),
                                ..Default::default()
                            };

                            let _ = client.get(spec).await;
                            drop(permit);
                        }
                    }
                    .instrument(debug_span!("runner", c, s)),
                );
            }
        }
    }
}
