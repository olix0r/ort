use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tracing::debug;

#[derive(Copy, Clone)]
pub struct RateLimit(Option<Inner>);

#[derive(Copy, Clone)]
struct Inner {
    requests: usize,
    window: Duration,
}

#[derive(Clone)]
pub struct Acquire(Option<Arc<Semaphore>>);

pub struct Permit(Option<tokio::sync::OwnedSemaphorePermit>);

// === impl RateLimit ===

impl Default for RateLimit {
    fn default() -> Self {
        Self::unlimited()
    }
}

impl RateLimit {
    pub fn new(requests: usize, window: Duration) -> Self {
        if requests == 0 || (window.as_secs() == 0 && window.subsec_nanos() == 0) {
            Self::unlimited()
        } else {
            Self(Some(Inner { requests, window }))
        }
    }

    pub fn unlimited() -> Self {
        Self(None)
    }

    pub fn spawn(self) -> Acquire {
        let Inner { requests, window } = match self.0 {
            None => return Acquire(None),
            Some(inner) => inner,
        };

        let semaphore = Arc::new(Semaphore::new(requests));
        let acquire = Acquire(Some(semaphore.clone()));

        tokio::spawn(async move {
            loop {
                // Wait for the window to expire befor adding more permits.
                tokio::time::delay_for(window).await;

                // If all of the acquire handles have been dropped, stop running.
                if Arc::strong_count(&semaphore) == 1 {
                    return;
                }

                // Refill the semaphore up to `requests`.
                let permits = requests - semaphore.available_permits();
                debug!(permits, "Refilling rate limit");
                semaphore.add_permits(permits);
            }
        });

        acquire
    }
}

// === impl Acquire ===

impl Acquire {
    pub async fn acquire(&self) -> Permit {
        match self.0.clone() {
            None => Permit(None),
            Some(s) => {
                let p = s.acquire_owned().await;
                Permit(Some(p))
            }
        }
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
