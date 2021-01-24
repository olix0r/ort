use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[async_trait::async_trait]
pub trait Acquire: Clone + Send + Sync + 'static {
    type Handle: Send + Sync + 'static;

    async fn acquire(&self) -> Self::Handle;
}

#[derive(Clone)]
pub struct Limit<A, M> {
    acquire: A,
    inner: M,
}

// === impl Limit ===

impl<A, T> Limit<A, T> {
    pub fn new(acquire: A, inner: T) -> Self {
        Self { acquire, inner }
    }
}

#[async_trait::async_trait]
impl<T, A, M> crate::MakeOrt<T> for Limit<A, M>
where
    T: Send + 'static,
    A: Acquire,
    M: crate::MakeOrt<T>,
    M::Ort: crate::Ort,
{
    type Ort = Limit<A, M::Ort>;

    async fn make_ort(&mut self, target: T) -> Result<Self::Ort, crate::Error> {
        let inner = self.inner.make_ort(target).await?;
        let acquire = self.acquire.clone();
        Ok(Limit { inner, acquire })
    }
}

#[async_trait::async_trait]
impl<A, O> crate::Ort for Limit<A, O>
where
    A: Acquire,
    O: crate::Ort,
{
    async fn ort(&mut self, spec: crate::Spec) -> Result<crate::Reply, crate::Error> {
        let permit = self.acquire.acquire().await;
        let reply = self.inner.ort(spec).await;
        drop(permit);
        reply
    }
}

// === impl Acqire ===

#[async_trait::async_trait]
impl<A: Acquire, B: Acquire> Acquire for (A, B) {
    type Handle = (A::Handle, B::Handle);

    async fn acquire(&self) -> Self::Handle {
        tokio::join!(self.0.acquire(), self.1.acquire())
    }
}

#[async_trait::async_trait]
impl<A: Acquire> Acquire for Option<A> {
    type Handle = Option<A::Handle>;

    async fn acquire(&self) -> Self::Handle {
        match self {
            Some(semaphore) => Some(semaphore.acquire().await),
            None => None,
        }
    }
}

#[async_trait::async_trait]
impl Acquire for Arc<Semaphore> {
    type Handle = OwnedSemaphorePermit;

    async fn acquire(&self) -> Self::Handle {
        self.clone().acquire_owned().await.expect("Semaphore must not be closed")
    }
}
