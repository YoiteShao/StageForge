use crate::agent::{Agent, AgentStage, StageStatus};
use crate::db::{DatabaseManager, DbStore};
use crate::stage::Stage;
use crate::utils::try_lock_with_retry;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tracing::{debug, error, info, warn, Instrument};

#[derive(Debug)]
pub struct Pipeline<M>
where
    M: DatabaseManager,
    M::Store: DbStore<Status = StageStatus, Data = Agent>,
{
    stage_router: Arc<HashMap<AgentStage, mpsc::Sender<Arc<Mutex<Agent>>>>>,
    total_agents: Arc<AtomicUsize>,
    completed_agents: Arc<AtomicUsize>,
    completion_notify: Arc<Notify>,
    is_terminated: Arc<AtomicBool>,
    db_manager: Arc<M>,
}

impl<M> Clone for Pipeline<M>
where
    M: DatabaseManager,
    M::Store: DbStore<Status = StageStatus, Data = Agent>,
{
    fn clone(&self) -> Self {
        Self {
            stage_router: self.stage_router.clone(),
            total_agents: self.total_agents.clone(),
            completed_agents: self.completed_agents.clone(),
            completion_notify: self.completion_notify.clone(),
            is_terminated: self.is_terminated.clone(),
            db_manager: self.db_manager.clone(),
        }
    }
}

impl<M> Drop for Pipeline<M>
where
    M: DatabaseManager,
    M::Store: DbStore<Status = StageStatus, Data = Agent>,
{
    fn drop(&mut self) {
        self.stage_router = Arc::new(HashMap::new());
    }
}

impl<M> Pipeline<M>
where
    M: DatabaseManager,
    M::Store: DbStore<Status = StageStatus, Data = Agent>,
{
    pub async fn new(db_manager: M) -> Result<Self> {
        Ok(Self {
            stage_router: Arc::new(HashMap::new()),
            total_agents: Arc::new(AtomicUsize::new(0)),
            completed_agents: Arc::new(AtomicUsize::new(0)),
            completion_notify: Arc::new(Notify::new()),
            is_terminated: Arc::new(AtomicBool::new(false)),
            db_manager: Arc::new(db_manager),
        })
    }

    pub async fn register_stages(&mut self, stages: Vec<Box<dyn Stage>>) -> Result<()> {
        let mut stage_rx_vec = vec![];
        for stage in stages.iter() {
            let stage_type = stage.stage();
            let is_exist = self.stage_router.contains_key(&stage_type);
            if is_exist {
                warn!("Attempt to register an existing stage: {:?}", stage_type);
                return Err(anyhow!(
                    "Stage {:?} already registered, skipping",
                    stage_type
                ));
            }
            debug!(
                "Register stage {:?}, concurrency={}, timeout={:?}, retry_times={}",
                stage_type,
                stage.concurrency_limit(),
                stage.timeout(),
                stage.retry_times()
            );
            let buffer_size = if stage_rx_vec.len() == 0 {
                100
            } else {
                stage.concurrency_limit() * 2
            };
            let (stage_buffer_tx, stage_buffer_rx) = mpsc::channel(buffer_size);
            let mut mirror_router = (*self.stage_router).clone();
            mirror_router.insert(stage_type, stage_buffer_tx);
            self.stage_router = Arc::new(mirror_router);
            stage_rx_vec.push(stage_buffer_rx);
        }
        debug!(
            "Pipeline router: {:?}",
            self.stage_router.keys().collect::<Vec<_>>()
        );
        for (index, stage_buffer_rx) in stage_rx_vec.into_iter().enumerate() {
            let pipeline = Arc::new(self.clone());
            let stage = stages[index].clone_box();
            pipeline.spawn_stage_worker(stage_buffer_rx, stage);
        }

        Ok(())
    }

    fn spawn_stage_worker(
        self: Arc<Self>,
        stage_buffer_rx: mpsc::Receiver<Arc<Mutex<Agent>>>,
        stage: Box<dyn Stage>,
    ) {
        let stage_buffer_rx = Arc::new(tokio::sync::Mutex::new(stage_buffer_rx));
        let stage_type = stage.stage();

        for worker_id in 0..stage.concurrency_limit() {
            let worker_rx = stage_buffer_rx.clone();
            let stage = stage.clone_box();

            let task_name = format!("{}_worker_{}", stage_type, worker_id);
            let span = tracing::debug_span!("task", name = %task_name);

            let self_clone = self.clone();
            tokio::spawn(
                async move {
                    info!(
                        "Start worker{} thread for stage {:?}",
                        worker_id, stage_type
                    );

                    while let Some(agent) = {
                        let mut rx_lock = worker_rx.lock().await;
                        rx_lock.recv().await
                    } {
                        if self_clone.is_terminated.load(Ordering::SeqCst) {
                            info!("terminated one agent");
                            self_clone.add_done_agent_count().await;
                            continue;
                        }
                        // process agent
                        let (next_stage, mut process_status) = match self_clone
                            .process_agent(
                                agent.clone(),
                                stage.clone_box(),
                                self_clone.is_terminated.clone(),
                            )
                            .await
                        {
                            Ok((next_stage, process_status)) => (next_stage, process_status),
                            Err(e) => {
                                error!("Error processing agent: {} {}", stage_type, e);
                                (
                                    None,
                                    StageStatus::Failed(format!("{} failed: {}", stage_type, e)),
                                )
                            }
                        };

                        if let Some(next_stage) = next_stage {
                            if let Some(stage_sender) = self_clone.stage_router.get(&next_stage) {
                                if let Ok(_) = stage_sender.send(agent.clone()).await {
                                    continue;
                                } else {
                                    error!("Failed to send agent to next stage: {}", next_stage);
                                    process_status = StageStatus::Failed(format!(
                                        "failed in transfering to: {}",
                                        next_stage
                                    ));
                                }
                            } else {
                                error!("No {} stage registered for agent", next_stage);
                                process_status = StageStatus::Failed(format!(
                                    "No {} stage registered",
                                    next_stage
                                ));
                            }
                        }

                        if let StageStatus::Completed(_) = process_status {
                            match try_lock_with_retry(&agent, &format!("set agent to be done"))
                                .await
                            {
                                Ok(agent_handler) => {
                                    let agent_id = agent_handler.id.clone();
                                    let _ = self_clone
                                        .db_manager
                                        .update_data(agent_id.clone(), StageStatus::Done)
                                        .await
                                        .map_err(|e| {
                                            error!(
                                                "Failed to update agent {} status to Done: {}",
                                                agent_id, e
                                            );
                                            e
                                        });
                                }
                                Err(e) => {
                                    error!("Failed to lock {} agent: {}", stage_type, e);
                                }
                            }
                        }
                        self_clone.add_done_agent_count().await;
                    }
                    debug!(
                        "{} worker {} thread stopped due to last worker tx closed",
                        stage_type, worker_id
                    );
                }
                .instrument(span),
            );
        }
    }

    async fn process_agent(
        &self,
        agent: Arc<Mutex<Agent>>,
        stage: Box<dyn Stage>,
        is_terminated: Arc<AtomicBool>,
    ) -> Result<(Option<AgentStage>, StageStatus)> {
        let stage_type = stage.stage();
        let mut agent_id = String::from("Unknown");
        // before process agent, set agent status
        match try_lock_with_retry(&agent, &format!("before process agent")).await {
            Ok(mut agent_handler) => {
                agent_id = agent_handler.id.clone();
                let _ = self
                    .db_manager
                    .update_data(
                        agent_id.clone(),
                        StageStatus::Processing(format!("{} processing", stage_type)),
                    )
                    .await
                    .map_err(|e| {
                        error!(
                            "Failed to update agent {} status to processing: {}",
                            agent_id, e
                        );
                        e
                    });
            }
            Err(e) => {
                error!("Failed to lock {} agent: {}", stage_type, e);
            }
        }

        // get process status
        let result = stage.run(agent.clone(), is_terminated.clone()).await;
        let mut next_stage = None;
        let process_status = match result {
            Ok(_) => {
                debug!("agent {} {} completed", agent_id, stage_type);
                StageStatus::Completed(format!("{} completed", stage_type))
            }
            Err(e) => {
                error!("agent {} {} failed: {}", agent_id, stage_type, e);
                StageStatus::Failed(format!("{} failed: {}", stage_type, e))
            }
        };

        // after process agent, set agent status and return next stage
        match try_lock_with_retry(&agent, &format!("after process agent")).await {
            Ok(mut agent_handler) => {
                agent_handler.set_stage_flag(stage_type);
                next_stage = match process_status {
                    StageStatus::Completed(_) => agent_handler.get_next_stage(),
                    _ => {
                        debug!(
                            "agent {} is failed in {} and no next stage",
                            agent_id, stage_type
                        );
                        None
                    }
                };
                let _ = self
                    .db_manager
                    .update_data(agent_id.clone(), process_status.clone())
                    .await
                    .map_err(|e| {
                        error!(
                            "Failed to update agent {} status to {:?}: {}",
                            agent_id, process_status, e
                        );
                        e
                    });
            }
            Err(e) => {
                error!("Failed to lock agent: {}", e);
            }
        };

        debug!("agent {} next stage: {:?}", agent_id, next_stage);
        Ok((next_stage, process_status))
    }

    async fn add_done_agent_count(&self) {
        self.completed_agents.fetch_add(1, Ordering::SeqCst);
        debug!(
            "Completed agents count: {}, total agents count: {}",
            self.completed_agents.load(Ordering::SeqCst),
            self.total_agents.load(Ordering::SeqCst)
        );
        if self.completed_agents.load(Ordering::SeqCst) == self.total_agents.load(Ordering::SeqCst)
        {
            self.completion_notify.notify_waiters();
        }
    }

    pub async fn submit_agent(&self, agent: Agent) -> Result<()> {
        // Save agent to store
        let agent_id = agent.id.clone();
        if let Err(e) = self.db_manager.insert_data(&agent).await {
            error!("Failed to insert agent {} to store: {}", agent_id, e);
            return Err(anyhow!("Failed to insert agent to store: {}", e));
        }

        if let Some(current_stage) = agent.get_next_stage() {
            if let Some(stage_sender) = self.stage_router.get(&current_stage) {
                let agent = Arc::new(Mutex::new(agent));
                match stage_sender.send(agent).await {
                    Ok(_) => {
                        self.total_agents.fetch_add(1, Ordering::SeqCst);
                        debug!(
                            "Submit agent {} to pipeline stage {}, total agents: {}",
                            agent_id,
                            current_stage,
                            self.total_agents.load(Ordering::SeqCst)
                        );
                    }
                    Err(e) => {
                        error!("Failed to send agent {} to buffer stage: {}", agent_id, e);
                    }
                }
            } else {
                error!("{} stage not registered", current_stage);
            }
        } else {
            error!("agent {} has no next stage", agent_id);
        };
        Ok(())
    }

    pub fn is_all_agents_completed(&self) -> bool {
        let completed = self.completed_agents.load(Ordering::SeqCst);
        let total = self.total_agents.load(Ordering::SeqCst);
        completed == total
    }

    pub async fn terminate_pipeline(&mut self) -> Result<()> {
        if !self.is_all_agents_completed() {
            warn!("Attempt to clean pipeline while agents are still processing");
        }
        self.is_terminated.store(true, Ordering::SeqCst);
        let _ = self.wait_for_completion().await;
        info!("Pipeline terminated successfully");
        Ok(())
    }

    pub async fn get_completion_status(&self) -> (usize, usize) {
        let completed = self.completed_agents.load(Ordering::SeqCst);
        let total = self.total_agents.load(Ordering::SeqCst);
        (completed, total)
    }

    pub async fn init_pipeline(&mut self) {
        self.completed_agents.store(0, Ordering::SeqCst);
        self.total_agents.store(0, Ordering::SeqCst);
        self.is_terminated.store(false, Ordering::SeqCst);
    }

    pub async fn wait_for_completion(&self) -> Result<()> {
        while !self.is_all_agents_completed() {
            self.completion_notify.notified().await;
        }
        Ok(())
    }
}
