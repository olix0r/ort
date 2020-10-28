use crate::{proto, Client, MakeClient};
use hdrhistogram::sync::Recorder;

#[derive(Clone)]
pub struct MakeMetrics<M> {
    inner: M,
    recorder: Recorder<u64>,
}

#[derive(Clone)]
pub struct Metrics<C> {
    inner: C,
}

impl<M> MakeMetrics<M> {
    pub fn new(inner: M, recorder: Recorder<u64>) -> Self {
        Self { inner, recorder }
    }
}

#[async_trait::async_trait]
impl<M> MakeClient for MakeMetrics<M>
where
    M: MakeClient + Send + 'static,
    M::Client: Send + 'static,
{
    type Client = Metrics<M::Client>;

    async fn make_client(&mut self) -> Self::Client {
        let inner = self.inner.make_client().await;
        Metrics { inner }
    }
}

#[async_trait::async_trait]
impl<C: Client + Send + 'static> Client for Metrics<C> {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status> {
        self.inner.get(spec).await
    }
}
