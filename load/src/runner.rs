use crate::{limit::Acquire, proto, Client, Distribution, MakeClient, Target};
use rand::{distributions::Distribution as _, rngs::SmallRng};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tracing::{debug, debug_span, info, trace, warn};
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct Runner<L> {
    requests_per_target: usize,
    limit: L,
    total_requests: Countdown,
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
        requests_per_target: usize,
        total_requests: usize,
        limit: L,
        response_sizes: Arc<Distribution>,
        rng: SmallRng,
    ) -> Self {
        let total_requests = Countdown::new(total_requests);
        Self {
            requests_per_target,
            limit,
            total_requests,
            response_sizes,
            rng,
        }
    }

    pub async fn run<C>(self, mut connect: C, targets: Vec<Target>)
    where
        C: MakeClient<Target> + Clone + Send + 'static,
        C::Client: Clone + Send + 'static,
    {
        let Self {
            requests_per_target,
            limit,
            total_requests,
            response_sizes,
            rng,
        } = self;

        if requests_per_target == 0 && targets.len() > 1 {
            warn!("No request-per-target limit. All but the first target will be ignored");
        }

        for target in targets.into_iter().cycle() {
            let requests_per_target = Countdown::new(requests_per_target);
            let client = connect.make_client(target.clone()).await;

            let limit = limit.clone();
            let requests_per_target = requests_per_target.clone();
            let response_sizes = response_sizes.clone();
            let total_requests = total_requests.clone();
            let mut rng = rng.clone();
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

                    // TODO generate request params (latency, error).
                    let rsp_sz = response_sizes.sample(&mut rng);
                    let mut client = client.clone();
                    tokio::spawn(
                        async move {
                            let spec = proto::ResponseSpec {
                                result: Some(proto::response_spec::Result::Success(
                                    proto::response_spec::Success {
                                        size: rsp_sz as i64,
                                    },
                                )),
                                ..proto::ResponseSpec::default()
                            };
                            trace!(%rsp_sz, request = %r, "Sending request");
                            match client.get(spec).await {
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
            .instrument(debug_span!("client", %target))
            .await;

            debug!(%target, "Complete");
        }
    }
}
