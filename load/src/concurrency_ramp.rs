use super::Ramp;
use ort_core::limit;
use std::sync::{Arc, Weak};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time,
};
use tracing::{debug, info};

#[derive(Clone)]
pub(crate) struct ConcurrencyRamp(Arc<Semaphore>);

// === impl ConcurrencyRamp ===

impl ConcurrencyRamp {
    pub(crate) fn spawn(ramp: Ramp) -> Self {
        let sem = Arc::new(Semaphore::new(ramp.init()));
        if ramp.init() != ramp.max {
            info!(?ramp, "Spawning concurrency limit");
            let weak = Arc::downgrade(&sem);
            tokio::spawn(run(ramp, weak));
        } else {
            info!(limit = %ramp.init(), "Fixed concurrency limit");
        }
        Self(sem)
    }
}

async fn run(ramp: Ramp, weak: Weak<Semaphore>) {
    // Figure out how frequently to increase the concurrency and create a timer.
    debug_assert!(ramp.max > ramp.min && ramp.min_step != 0);
    let updates = (ramp.max - ramp.min) / ramp.min_step;

    let mut interval = {
        let t = time::Duration::from_millis(ramp.period.as_millis() as u64 / updates as u64);
        time::interval_at(time::Instant::now() + t, t)
    };

    // The concurrency is already at the minimum. Wait for the timer to fire and increment it until
    // we're at the maximum concurrency.
    let mut concurrency = ramp.min;

    // Spend the first tick at the initial concurrency.
    interval.tick().await;

    loop {
        for _ in 0..updates {
            let sem = match weak.upgrade() {
                Some(sem) => sem,
                None => {
                    debug!("Terminating task");
                    return;
                }
            };
            sem.add_permits(ramp.min_step);
            concurrency += ramp.min_step;
            debug!(%concurrency, "Increased concurrency");
            interval.tick().await;
        }

        if concurrency < ramp.max {
            let sem = match weak.upgrade() {
                Some(sem) => sem,
                None => {
                    debug!("Terminating task");
                    return;
                }
            };
            sem.add_permits(ramp.max - concurrency);
            concurrency = ramp.max;
            debug!(%concurrency, "Increased concurrency");
            interval.tick().await;
        }

        if !ramp.reset {
            return;
        }
        concurrency = ramp.min;
        debug!(%concurrency, "Resetting concurrency");
        let sem = match weak.upgrade() {
            Some(sem) => sem,
            None => {
                debug!("Terminating task");
                return;
            }
        };
        let permits = match sem.acquire_many((ramp.max - ramp.min) as u32).await {
            Ok(p) => p,
            Err(e) => {
                debug!(%e, "Failed to acquire permits");
                return;
            }
        };
        permits.forget();
        interval.tick().await;
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
