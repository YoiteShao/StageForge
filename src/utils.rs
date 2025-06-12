use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

pub async fn try_lock_with_retry<'a, T>(
    mutex: &'a Arc<Mutex<T>>,
    context: &str,
) -> Result<tokio::sync::MutexGuard<'a, T>, String> {
    match mutex.try_lock() {
        Ok(guard) => Ok(guard),
        Err(_) => {
            tracing::warn!(
                "Failed to acquire lock in {}, retrying after 5s...",
                context
            );

            sleep(Duration::from_secs(5)).await;

            match mutex.try_lock() {
                Ok(guard) => Ok(guard),
                Err(_) => {
                    tracing::error!("Failed to acquire lock in {} after retry", context);
                    Err(format!("Failed to acquire lock in {}", context))
                }
            }
        }
    }
}
