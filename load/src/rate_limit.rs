use std::sync::{Arc, Weak};
use tokio::{
    sync::Semaphore,
    time::{interval_at, Duration, Instant},
};
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
        match self.0 {
            None => Acquire(None),
            Some(inner) => {
                // Initialize the semaphore permitting requests.
                let sem = Arc::new(Semaphore::new(inner.requests));
                let weak = Arc::downgrade(&sem);
                tokio::spawn(inner.run(weak));
                Acquire(Some(sem))
            }
        }
    }
}

impl Inner {
    async fn run(self, weak: Weak<Semaphore>) {
        let mut interval = interval_at(Instant::now() + self.window, self.window);
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
                    let permits = self.requests - sem.available_permits();
                    debug!(permits, "Refilling rate limit");
                    sem.add_permits(permits);
                }
            }
        }
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
