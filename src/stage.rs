use crate::agent::Agent;
use crate::agent::AgentStage;
use async_trait::async_trait;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};
use tracing::error;

#[async_trait]
pub trait Stage: Send + Sync + 'static {
    fn concurrency_limit(&self) -> usize;
    fn timeout(&self) -> Duration;
    fn retry_times(&self) -> u32;
    fn stage(&self) -> AgentStage;
    fn clone_box(&self) -> Box<dyn Stage>;

    fn process(&self, _agent: &mut Agent, _is_terminated: Arc<AtomicBool>) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("Sync process not implemented"))
    }

    async fn async_process(
        &self,
        _agent: &mut Agent,
        _is_terminated: Arc<AtomicBool>,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("Async process not implemented"))
    }

    async fn async_process_wrapper(
        &self,
        agent: Arc<Mutex<Agent>>,
        is_terminated: Arc<AtomicBool>,
    ) -> anyhow::Result<()> {
        let this = self.clone_box();

        if let Ok(mut agent) = agent.try_lock() {
            match this.async_process(&mut *agent, is_terminated.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if !e.to_string().contains("Async process not implemented") {
                        return Err(e);
                    }
                }
            }
        }

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let mut agent = agent.blocking_lock();
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                this.process(&mut *agent, is_terminated.clone())
            }))
            .map_err(|e| {
                let panic_msg = e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| e.downcast_ref::<&str>().map(|s| *s))
                    .unwrap_or("Unknown panic message");
                error!("Task panicked: {}", panic_msg);
                anyhow::anyhow!("Task panicked: {}", panic_msg)
            })?;
            result
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    #[allow(unused_variables, unreachable_code, unused_mut)]
    async fn run(
        &self,
        agent: Arc<Mutex<Agent>>,
        is_terminated: Arc<AtomicBool>,
    ) -> anyhow::Result<()> {
        let mut attempts = 0;
        let retry_delay = Duration::from_secs(1);
        let stage_type = self.stage();
        let max_retries = self.retry_times();

        loop {
            attempts += 1;
            match timeout(
                self.timeout(),
                self.async_process_wrapper(agent.clone(), is_terminated.clone()),
            )
            .await
            {
                Ok(result) => match result {
                    Ok(_) => {
                        return Ok(());
                    }
                    Err(e) => {
                        if attempts >= max_retries {
                            return Err(anyhow::anyhow!(
                                "Stage {} failed after {} attempts: {}",
                                stage_type,
                                attempts,
                                e
                            ));
                        }

                        tracing::warn!(
                            "Stage {} attempt {} failed: {}, retrying...",
                            stage_type,
                            attempts,
                            e
                        );
                        sleep(retry_delay).await;
                    }
                },
                Err(_) => {
                    if attempts >= max_retries {
                        return Err(anyhow::anyhow!(
                            "Stage {} timed out after {} attempts",
                            stage_type,
                            attempts
                        ));
                    }

                    tracing::warn!(
                        "Stage {} attempt {} timed out, retrying...",
                        stage_type,
                        attempts
                    );
                    sleep(retry_delay).await;
                }
            }
        }
    }
}
