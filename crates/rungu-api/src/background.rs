//! Fire-and-forget side-effect tasks (webhook delivery, analytics writes)
//! with an in-flight count, so short-lived processes can wait for them.
//!
//! The HTTP server never needs to wait. `rungu mcp` does: when stdin closes
//! the runtime is dropped, which would cancel a webhook still in delivery.

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// Decrements on drop, so a panicking or cancelled task still counts down.
struct InFlight;

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Spawn a detached side-effect task that [`drain`] can wait for.
pub fn spawn<F>(task: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
    let guard = InFlight;
    tokio::spawn(async move {
        let _guard = guard;
        task.await;
    });
}

/// Wait until every spawned side effect has finished, or `timeout` passes.
/// Returns how many were still running when it gave up (0 = all done).
pub async fn drain(timeout: Duration) -> usize {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let pending = IN_FLIGHT.load(Ordering::SeqCst);
        if pending == 0 || tokio::time::Instant::now() >= deadline {
            return pending;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drain_waits_for_spawned_tasks() {
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = done.clone();
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            flag.store(true, Ordering::SeqCst);
        });
        assert_eq!(drain(Duration::from_secs(5)).await, 0);
        assert!(done.load(Ordering::SeqCst));
    }
}
