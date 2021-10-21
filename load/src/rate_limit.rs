use super::Ramp;
use ort_core::limit;
use std::sync::{Arc, Weak};
use tokio::{sync::Semaphore, time};
use tracing::debug;

#[derive(Clone)]
pub(crate) struct RateLimit(Arc<Semaphore>);

pub struct Permit(Option<tokio::sync::OwnedSemaphorePermit>);

// === impl RateLimit ===

impl RateLimit {
    pub fn spawn(ramp: Ramp, window: time::Duration) -> Option<Self> {
        if ramp.max > 0 && window > time::Duration::new(0, 0) {
            // Initialize the semaphore permitting requests.
            let sem = Arc::new(Semaphore::new(ramp.init()));
            let weak = Arc::downgrade(&sem);
            tokio::spawn(run(ramp, window, weak));
            Some(Self(sem))
        } else {
            None
        }
    }
}

fn step(ramp: Ramp, interval: time::Duration) -> usize {
    if ramp.period.as_secs() == 0 || interval.as_secs() == 0 {
        return 0;
    }

    let delta = ramp.max - ramp.min;
    let steps = (ramp.period.as_secs() / interval.as_secs()) as usize;
    if steps == 0 {
        return 0;
    }
    delta / steps
}

async fn run(ramp: Ramp, window: time::Duration, weak: Weak<Semaphore>) {
    let mut limit = ramp.init();
    let step = step(ramp, window);

    let mut interval = time::interval_at(time::Instant::now() + window, window);
    loop {
        // Wait for the window to expire before adding more permits.
        interval.tick().await;

        if limit < ramp.max {
            limit = (limit + step).min(ramp.max);
        } else if ramp.reset {
            limit = ramp.init();
        }

        // Refill the semaphore up to `limit`. If all of the acquire handles have been dropped, stop
        // running.
        match weak.upgrade() {
            None => {
                debug!("Terminating task");
                return;
            }
            Some(sem) => {
                let permits = limit - sem.available_permits();
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
