use hdrhistogram as hdr;
use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
use parking_lot::RwLock;
use std::{sync::Arc, time::Instant};
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
impl<M, T> MakeOrt<T> for MakeMetrics<M>
where
    T: Send + 'static,
    M: MakeOrt<T> + Send + 'static,
    M::Ort: Send + 'static,
{
    type Ort = Metrics<M::Ort>;

    async fn make_ort(&mut self, t: T) -> Result<Self::Ort, Error> {
        let inner = self.inner.make_ort(t).await?;
        let histogram = self.histogram.clone();
        Ok(Metrics { inner, histogram })
    }
}

#[async_trait::async_trait]
impl<C: Ort + Send + 'static> Ort for Metrics<C> {
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        let t0 = Instant::now();
        let res = self.inner.ort(spec).await;
        let elapsed = Instant::now() - t0;
        let micros = elapsed.as_micros();
        trace!(%micros);
        let mut h = self.histogram.write();
        if micros < std::u64::MAX as u128 {
            h.saturating_record(micros as u64);
        } else {
            h.saturating_record(std::u64::MAX);
        }
        res
    }
}
