use ort_core::limit;
use std::sync::{Arc, Weak};
use tokio::{sync::Semaphore, time};
use tracing::debug;

#[derive(Clone)]
pub struct RateLimit(Arc<Semaphore>);

pub struct Permit(Option<tokio::sync::OwnedSemaphorePermit>);

// === impl RateLimit ===

impl RateLimit {
    pub fn spawn(requests: usize, window: time::Duration) -> Option<Self> {
        if requests > 0 && window > time::Duration::new(0, 0) {
            // Initialize the semaphore permitting requests.
            let sem = Arc::new(Semaphore::new(requests));
            let weak = Arc::downgrade(&sem);
            tokio::spawn(run(requests, window, weak));
            Some(Self(sem))
        } else {
            None
        }
    }
}

async fn run(requests: usize, window: time::Duration, weak: Weak<Semaphore>) {
    let mut interval = time::interval_at(time::Instant::now() + window, window);
    loop {
        // Wait for the window to expire befor adding more permits.
        interval.tick().await;

        // Refill the semaphore up to `requests`. If all of the acquire handles have been
        // dropped, stop running.
        match weak.upgrade() {
            None => {
                debug!("Terminating task");
                return;
            }
            Some(sem) => {
                let permits = requests - sem.available_permits();
                debug!(permits, "Refilling rate limit");
                sem.add_permits(permits);
            }
        }
    }
}

// === impl Acquire ===

#[async_trait::async_trait]
impl limit::Acquire for RateLimit {
    type Handle = Permit;

    async fn acquire(&self) -> Permit {
        let p = self
            .0
            .clone()
            .acquire_owned()
            .await
            .expect("Semaphore must not close");
        Permit(Some(p))
    }
}

// === impl Permit ===

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            p.forget()
        }
    }
}
