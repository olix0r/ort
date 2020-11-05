use crate::{proto, Client, MakeClient};
use tokio::time;

#[derive(Clone)]
pub struct MakeRequestTimeout<M> {
    inner: M,
    timeout: time::Duration,
}

#[derive(Clone)]
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
impl<M, T> MakeClient<T> for MakeRequestTimeout<M>
where
    T: Send + 'static,
    M: MakeClient<T> + Send + 'static,
    M::Client: Send + 'static,
{
    type Client = RequestTimeout<M::Client>;

    async fn make_client(&mut self, t: T) -> Self::Client {
        let inner = self.inner.make_client(t).await;
        let timeout = self.timeout.clone();
        RequestTimeout { inner, timeout }
    }
}

#[async_trait::async_trait]
impl<C: Client + Send + 'static> Client for RequestTimeout<C> {
    async fn get(
        &mut self,
        spec: proto::ResponseSpec,
    ) -> Result<proto::ResponseReply, tonic::Status> {
        match time::timeout(self.timeout, self.inner.get(spec)).await {
            Ok(res) => res,
            Err(_) => Err(tonic::Status::deadline_exceeded("request timeout")),
        }
    }
}
