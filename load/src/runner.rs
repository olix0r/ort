use crate::{limit::Acquire, proto, Client, Distribution, MakeClient, Target};
use rand::{distributions::Distribution as _, rngs::SmallRng, seq::SliceRandom};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tracing::{debug_span, trace};

#[derive(Clone)]
pub struct Runner<L> {
    requests_per_client: usize,
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
        requests_per_client: usize,
        total_requests: usize,
        limit: L,
        response_sizes: Arc<Distribution>,
        rng: SmallRng,
    ) -> Self {
        let total_requests = if total_requests == 0 {
            None
        } else {
            Some(TotalRequestsLimit {
                limit: total_requests,
                issued: Arc::new(0.into()),
            })
        };
        Self {
            requests_per_client,
            limit,
            total_requests,
            response_sizes,
            rng,
        }
    }

    pub async fn run<C>(self, mut connect: C, mut targets: Vec<Target>)
    where
        C: MakeClient<Target> + Clone + Send + 'static,
        C::Client: Clone + Send + 'static,
    {
        let Self {
            requests_per_client,
            limit,
            total_requests,
            response_sizes,
            mut rng,
        } = self;

        targets.shuffle(&mut rng);
        for target in targets.into_iter().cycle() {
            let span = debug_span!("client", %target);
            let _enter = span.enter();

            let client = connect.make_client(target.clone()).await;
            for _ in 0..requests_per_client {
                if let Some(lim) = total_requests.as_ref() {
                    if lim.issued.fetch_add(1, Ordering::Release) >= lim.limit {
                        return;
                    }
                }
                let permit = limit.acquire().await;
                trace!("Acquired permit");

                // TODO generate request params (latency, error).

                let rsp_sz = response_sizes.sample(&mut rng);

                let mut client = client.clone();
                tokio::spawn(async move {
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
                });
            }
        }
    }
}
