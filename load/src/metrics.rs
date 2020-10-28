use crate::{proto, Client, MakeClient};
use hdrhistogram::sync::Recorder;
use std::time::Instant;

#[derive(Clone)]
pub struct MakeMetrics<M> {
    inner: M,
    recorder: Recorder<u64>,
}

#[derive(Clone)]
pub struct Metrics<C> {
    inner: C,
    recorder: Recorder<u64>,
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
        let recorder = self.recorder.clone();
        Metrics { inner, recorder }
    }
}

#[async_trait::async_trait]
impl<C: Client + Send + 'static> Client for Metrics<C> {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status> {
        let t0 = Instant::now();
        let res = self.inner.get(spec).await;
        let elapsed = Instant::now() - t0;
        let micros = elapsed.as_micros();
        if micros < std::u64::MAX as u128 {
            self.recorder.saturating_record(micros as u64);
        } else {
            self.recorder.saturating_record(std::u64::MAX);
        }
        res
    }
}
