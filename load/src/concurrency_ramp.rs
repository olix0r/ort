use super::Ramp;
use ort_core::limit;
use std::sync::{Arc, Weak};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time,
};
use tracing::debug;

#[derive(Clone)]
pub(crate) struct ConcurrencyRamp(Arc<Semaphore>);

// === impl ConcurrencyRamp ===

impl ConcurrencyRamp {
    pub(crate) fn spawn(ramp: Ramp) -> Self {
        let sem = Arc::new(Semaphore::new(ramp.init()));
        if ramp.init() != ramp.max {
            let weak = Arc::downgrade(&sem);
            tokio::spawn(run(ramp, weak));
        }
        Self(sem)
    }
}

async fn run(ramp: Ramp, weak: Weak<Semaphore>) {
    // Figure out how frequently to increase the concurrency and create a timer.
    debug_assert!(ramp.max > ramp.min && ramp.min_step != 0);
    let updates = ((ramp.max - ramp.min) / ramp.min_step) - 1;
    let mut interval = {
        let interval = time::Duration::from_millis(ramp.period.as_millis() as u64 / updates as u64);
        time::interval_at(time::Instant::now() + interval, interval)
    };

    // The concurrency is already at the minimum. Wait for the timer to fire and increment it until
    // we're at the maximum concurrency.
    let mut concurrency = ramp.min;
    for _ in 0..updates {
        interval.tick().await;
        match weak.upgrade() {
            None => {
                debug!("Terminating task");
                return;
            }
            Some(sem) => {
                sem.add_permits(ramp.min_step);
                concurrency += ramp.min_step;
                debug!(%concurrency, "Increased concurrency");
            }
        }
    }
    if concurrency != ramp.max {
        interval.tick().await;
        if let Some(sem) = weak.upgrade() {
            sem.add_permits(ramp.max - concurrency);
            concurrency += ramp.max - concurrency;
            debug!(%concurrency, "Increased concurrency");
        }
    }
}

// === impl ConcurrencyRamp ===

#[async_trait::async_trait]
impl limit::Acquire for ConcurrencyRamp {
    type Handle = OwnedSemaphorePermit;

    async fn acquire(&self) -> OwnedSemaphorePermit {
        self.0
            .clone()
            .acquire_owned()
            .await
            .expect("Semaphore must not close")
    }
}
