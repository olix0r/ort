use bytes::BytesMut;
use ort_core::{latency, Error, Ort, Reply, Spec};
use rand::{distributions::Distribution, thread_rng, RngCore};
use tokio::time;
use tracing::trace;

#[derive(Clone)]
pub(crate) struct Replier {
    latencies: latency::Distribution,
}

impl Replier {
    pub fn new(latencies: latency::Distribution) -> Self {
        Self { latencies }
    }
}

#[async_trait::async_trait]
impl Ort for Replier {
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        let latency = spec.latency.max(self.latencies.sample(&mut thread_rng()));
        trace!(?latency, spec.response_size, "Serving request");
        let sleep = time::sleep(latency);
        let mut buf = BytesMut::with_capacity(spec.response_size);
        thread_rng().fill_bytes(buf.as_mut());
        sleep.await;
        trace!("Returning reply");
        Ok(Reply { data: buf.freeze() })
    }
}
