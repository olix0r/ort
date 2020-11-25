use crate::{Error, MakeOrt, Ort, Reply, Spec};
use std::sync::Arc;
use tokio::{sync::Mutex, time};
use tracing::debug;

#[derive(Clone)]
pub struct MakeReconnect<M> {
    inner: M,
    backoff: time::Duration,
    timeout: time::Duration,
}

#[derive(Clone)]
pub struct Reconnect<T, M, O> {
    target: T,
    make: MakeReconnect<M>,
    inner: Arc<Mutex<O>>,
}

impl<M> MakeReconnect<M> {
    pub fn new(inner: M, timeout: time::Duration, backoff: time::Duration) -> Self {
        Self {
            inner,
            timeout,
            backoff,
        }
    }
}

impl<M> MakeReconnect<M> {
    async fn make_inner<T>(&mut self, target: T) -> Result<M::Ort, Error>
    where
        T: Clone + Send + Sync + 'static,
        M: MakeOrt<T> + Clone,
    {
        let mut backoff = self.backoff;
        loop {
            let connect = self.inner.make_ort(target.clone());
            match time::timeout(self.timeout, connect).await {
                Ok(Ok(inner)) => return Ok(inner),
                _ => {}
            }

            time::delay_for(backoff).await;
            backoff *= 2;
        }
    }
}

#[async_trait::async_trait]
impl<T, M> MakeOrt<T> for MakeReconnect<M>
where
    T: Clone + Send + Sync + 'static,
    M: MakeOrt<T> + Clone,
{
    type Ort = Reconnect<T, M, M::Ort>;

    async fn make_ort(&mut self, target: T) -> Result<Self::Ort, Error> {
        let inner = self.make_inner(target.clone()).await?;
        return Ok(Reconnect {
            target,
            inner: Arc::new(Mutex::new(inner)),
            make: self.clone(),
        });
    }
}

#[async_trait::async_trait]
impl<T, M> Ort for Reconnect<T, M, M::Ort>
where
    T: Clone + Send + Sync + 'static,
    M: MakeOrt<T>,
{
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        let mut inner = self.inner.lock().await;
        loop {
            match inner.ort(spec.clone()).await {
                Ok(reply) => return Ok(reply),
                Err(error) => {
                    debug!(%error, "Rebuilding client");
                    *inner = self.make.make_inner(self.target.clone()).await?;
                }
            };
        }
    }
}
