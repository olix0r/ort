use crate::{proto, Client, MakeClient};
use hdrhistogram as hdr;
use std::{sync::Arc, time::Instant};
use tokio::sync::RwLock;
use tracing::trace;

#[derive(Clone)]
pub struct MakeMetrics<M> {
    inner: M,
    histogram: Arc<RwLock<hdr::Histogram<u64>>>,
}

#[derive(Clone)]
pub struct Metrics<C> {
    inner: C,
    histogram: Arc<RwLock<hdr::Histogram<u64>>>,
}

impl<M> MakeMetrics<M> {
    pub fn new(inner: M, histogram: Arc<RwLock<hdr::Histogram<u64>>>) -> Self {
        Self { inner, histogram }
    }
}

#[async_trait::async_trait]
impl<M, T> MakeClient<T> for MakeMetrics<M>
where
    T: Send + 'static,
    M: MakeClient<T> + Send + 'static,
    M::Client: Send + 'static,
{
    type Client = Metrics<M::Client>;

    async fn make_client(&mut self, t: T) -> Self::Client {
        let inner = self.inner.make_client(t).await;
        let histogram = self.histogram.clone();
        Metrics { inner, histogram }
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
        trace!(%micros);
        let mut h = self.histogram.write().await;
        if micros < std::u64::MAX as u128 {
            h.saturating_record(micros as u64);
        } else {
            h.saturating_record(std::u64::MAX);
        }
        res
    }
}
