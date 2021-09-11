use anyhow::{bail, Result};
use ort_core::limit;
use std::sync::{Arc, Weak};
use tokio::{sync::Semaphore, time};
use tracing::debug;

pub struct Ramp {
    min: usize,
    max: usize,
    period: time::Duration,
}

#[derive(Clone)]
pub struct RateLimit(Arc<Semaphore>);

pub struct Permit(Option<tokio::sync::OwnedSemaphorePermit>);

// === impl Ramp ===

impl From<usize> for Ramp {
    fn from(value: usize) -> Self {
        Self {
            min: value,
            max: value,
            period: time::Duration::from_secs(0),
        }
    }
}

impl Ramp {
    pub fn try_new(min: usize, max: usize, period: time::Duration) -> Result<Self> {
        if min > max {
            bail!("min must be <= max");
        }
        if period.as_secs() == 0 && min != max {
            bail!("period must be set if min != max")
        }
        Ok(Self { min, max, period })
    }

    fn init(&self) -> usize {
        if self.period.as_secs() == 0 {
            self.max
        } else {
            self.min
        }
    }

    fn step(&self, interval: time::Duration) -> usize {
        if self.period.as_secs() == 0 || interval.as_secs() == 0 {
            return 0;
        }

        let delta = self.max - self.min;
        let steps = (self.period.as_secs() / interval.as_secs()) as usize;
        delta / steps
    }
}

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

async fn run(ramp: Ramp, window: time::Duration, weak: Weak<Semaphore>) {
    let mut limit = ramp.init();
    let step = ramp.step(window);

    let mut interval = time::interval_at(time::Instant::now() + window, window);
    loop {
        // Wait for the window to expire before adding more permits.
        interval.tick().await;

        if limit < ramp.max {
            limit = (limit + step).min(ramp.max);
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
