use linkerd_metrics::{metrics, Counter, FmtMetrics, MillisAsSeconds, Summary};
use ort_core::{Error, MakeOrt, Ort, Reply, Spec};
use std::{fmt, sync::Arc};
use tokio::time;
use tracing::trace;

#[derive(Clone)]
pub struct MakeMetrics<M> {
    inner: M,
    shared: Arc<Shared>,
}

#[derive(Clone)]
pub struct Metrics<C> {
    inner: C,
    shared: Arc<Shared>,
}

struct Shared {
    latencies: Summary<MillisAsSeconds>,
    failures: Counter,
}

#[derive(Clone)]
pub struct Report(Arc<Shared>);

metrics! {
    response_latency_seconds: Summary<MillisAsSeconds> { "Response latencies" },
    response_failure_count: Counter { "A count of failed responses" }
}

impl FmtMetrics for Report {
    fn fmt_metrics(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        response_latency_seconds.fmt_help(f)?;
        response_latency_seconds.fmt_metric(f, &self.0.latencies)?;
        response_failure_count.fmt_help(f)?;
        response_failure_count.fmt_metric(f, &self.0.failures)?;
        Ok(())
    }
}

impl<M> MakeMetrics<M> {
    pub fn new(inner: M) -> (Self, Report) {
        let shared = Arc::new(Shared {
            failures: Counter::default(),
            latencies: Summary::new_resizable(10, time::Duration::from_secs(300), 5)
                .expect("Summary must be valid"),
        });
        let report = Report(shared.clone());
        (Self { inner, shared }, report)
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
        let shared = self.shared.clone();
        Ok(Metrics { inner, shared })
    }
}

#[async_trait::async_trait]
impl<C: Ort + Send + 'static> Ort for Metrics<C> {
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        let t0 = time::Instant::now();
        let res = self.inner.ort(spec).await;

        let millis = (time::Instant::now() - t0).as_millis();
        trace!(%millis);
        self.shared
            .latencies
            .record(millis as u64)
            .expect("latency must fit in histogram");

        if res.is_err() {
            self.shared.failures.incr();
        }
        res
    }
}
