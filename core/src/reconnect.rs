use crate::{Error, MakeOrt, Ort, Reply, Spec};
use tokio::time;

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
    inner: O,
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

#[async_trait::async_trait]
impl<T, M> MakeOrt<T> for MakeReconnect<M>
where
    T: Clone + Send + Sync + 'static,
    M: MakeOrt<T> + Clone,
{
    type Ort = Reconnect<T, M, M::Ort>;

    async fn make_ort(&mut self, target: T) -> Result<Self::Ort, Error> {
        let make = self.clone();
        let mut backoff = self.backoff;
        loop {
            let connect = self.inner.make_ort(target.clone());
            let timeout = time::delay_for(self.timeout);
            tokio::select! {
                res = connect => {
                    match res {
                        Ok(inner) => {
                            return Ok(Reconnect {
                                target,
                                inner,
                                make,
                            })
                        }
                        Err(_) => {
                            time::delay_for(backoff).await;
                            backoff *= 2;
                        }
                    }
                }
                _ = timeout => {
                    time::delay_for(backoff).await;
                    backoff *= 2;
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl<T, M> Ort for Reconnect<T, M, M::Ort>
where
    T: Clone + Send + Sync + 'static,
    M: MakeOrt<T>,
{
    async fn ort(&mut self, spec: Spec) -> Result<Reply, Error> {
        loop {
            self.inner = match self.inner.ort(spec.clone()).await {
                Ok(reply) => return Ok(reply),
                Err(_) => self.make.make_ort(self.target.clone()).await?.inner,
            };
        }
    }
}
