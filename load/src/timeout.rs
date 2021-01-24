use crate::{Error, MakeOrt, Ort, Reply, Spec};
use tokio::time;
use tracing::debug;

#[derive(Clone)]
pub struct MakeRequestTimeout<M> {
    inner: M,
    timeout: time::Duration,
}

#[derive(Clone, Debug)]
pub struct RequestTimeout<C> {
    inner: C,
    timeout: time::Duration,
}

impl<M> MakeRequestTimeout<M> {
    pub fn new(inner: M, timeout: time::Duration) -> Self {
        Self { inner, timeout }
    }
}

#[async_trait::async_trait]
impl<M, T> MakeOrt<T> for MakeRequestTimeout<M>
where
    T: Send + 'static,
    M: MakeOrt<T> + Send + 'static,
    M::Ort: Send + 'static,
{
    type Ort = RequestTimeout<M::Ort>;

    async fn make_ort(&mut self, t: T) -> Result<Self::Ort, Error> {
        let inner = self.inner.make_ort(t).await?;
        Ok(RequestTimeout {
            inner,
            timeout: self.timeout,
        })
    }
}

#[async_trait::async_trait]
impl<C: Ort + Send + 'static> Ort for RequestTimeout<C> {
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        match time::timeout(self.timeout, self.inner.ort(spec)).await {
            Ok(res) => res,
            Err(_) => {
                debug!(timeout.ms = %self.timeout.as_millis(), "Request timed out");
                Err(RequestTimeout {
                    inner: (),
                    timeout: self.timeout,
                }
                .into())
            }
        }
    }
}

impl std::fmt::Display for RequestTimeout<()> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Request timed out")
    }
}

impl std::error::Error for RequestTimeout<()> {}
