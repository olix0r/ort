use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::Mutex;
use tracing::debug;

#[derive(Clone)]
pub struct MakeRecycle<M> {
    inner: M,
    requests_per_client: Option<usize>,
}

#[derive(Clone)]
pub struct Recycle<T, M, O> {
    target: T,
    make: M,
    inner: Arc<Mutex<O>>,
    count: Arc<AtomicUsize>,
    requests_per_client: Option<usize>,
}

// === impl MakeRecycle ===

impl<M> MakeRecycle<M> {
    pub fn new(inner: M, requests_per_client: Option<usize>) -> Self {
        Self {
            inner,
            requests_per_client,
        }
    }
}

#[async_trait::async_trait]
impl<T, M> crate::MakeOrt<T> for MakeRecycle<M>
where
    T: Clone + Send + Sync + 'static,
    M: crate::MakeOrt<T> + Clone,
{
    type Ort = Recycle<T, M, M::Ort>;

    async fn make_ort(&mut self, target: T) -> Result<Self::Ort, crate::Error> {
        let inner = self.inner.make_ort(target.clone()).await?;
        return Ok(Recycle {
            target,
            make: self.inner.clone(),
            inner: Arc::new(Mutex::new(inner)),
            count: Default::default(),
            requests_per_client: self.requests_per_client,
        });
    }
}

// === impl Recycle ===

#[async_trait::async_trait]
impl<T, M> crate::Ort for Recycle<T, M, M::Ort>
where
    T: Clone + Send + Sync + 'static,
    M: crate::MakeOrt<T>,
{
    async fn ort(&mut self, spec: crate::Spec) -> Result<crate::Reply, crate::Error> {
        let req = self.count.fetch_add(1, Ordering::AcqRel);
        let mut inner = self.inner.lock().await;
        if let Some(rpc) = self.requests_per_client {
            if req % rpc == 0 {
                debug!(req, "Rebuilding client");
                *inner = self.make.make_ort(self.target.clone()).await?;
            }
        }
        inner.ort(spec).await
    }
}
