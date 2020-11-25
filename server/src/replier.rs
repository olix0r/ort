use bytes::BytesMut;
use ort_core::{Error, Ort, Reply, Spec};
use rand::{rngs::SmallRng, RngCore};
use tokio::time;

#[derive(Clone, Debug)]
pub(crate) struct Replier {
    rng: SmallRng,
}

impl Replier {
    pub fn new(rng: SmallRng) -> Self {
        Self { rng }
    }
}

#[async_trait::async_trait]
impl Ort for Replier {
    async fn ort(
        &mut self,
        Spec {
            latency,
            response_size,
        }: Spec,
    ) -> Result<Reply, Error> {
        let sleep = time::delay_for(latency);
        let mut buf = BytesMut::with_capacity(response_size);
        self.rng.fill_bytes(buf.as_mut());
        sleep.await;
        Ok(Reply { data: buf.freeze() })
    }
}
