use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[async_trait::async_trait]
pub trait Acquire: Clone + Send + Sync + 'static {
    type Handle: Send + Sync + 'static;

    async fn acquire(&self) -> Self::Handle;
}

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
        self.clone().acquire_owned().await
    }
}
