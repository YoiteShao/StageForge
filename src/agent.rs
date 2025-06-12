use crate::db::{DbData, IntoDbData};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentStage {}

impl fmt::Display for AgentStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct StageParam {
    pub stage_name: AgentStage,
    pub data: Box<dyn Any + Send + Sync>,
    pub flag: bool,
}
impl StageParam {
    pub fn set_flag(&mut self, flag: bool) {
        self.flag = flag;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StageStatus {
    Pending(String),
    Processing(String),
    Completed(String),
    Failed(String),
    Done,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    #[serde(flatten)]
    pub status: StageStatus,
    #[serde(skip)]
    pub stage_params: Vec<StageParam>,
}

impl Agent {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            status: StageStatus::Pending("Initialized".to_string()),

            stage_params: Vec::new(),
        }
    }

    pub fn add_stage_param<P: Any + Send + Sync>(&mut self, stage_param: StageParam) {
        self.stage_params.push(stage_param);
    }

    pub fn get_stage_param(&mut self, type_id: AgentStage) -> Option<&mut StageParam> {
        self.stage_params
            .iter_mut()
            .find(|p| p.stage_name == type_id)
    }

    pub fn set_status(&mut self, status: StageStatus) -> anyhow::Result<()> {
        self.status = status;
        Ok(())
    }
}

impl Agent {
    pub fn get_next_stage(&self) -> Option<AgentStage> {
        for stage_param in self.stage_params.iter() {
            if !stage_param.flag {
                return Some(stage_param.stage_name.clone());
            }
        }
        None
    }

    pub fn set_stage_flag(&mut self, stage_name: AgentStage) {
        self.stage_params
            .iter_mut()
            .find(|p| p.stage_name == stage_name)
            .unwrap()
            .flag = true;
    }
}

/// Database-specific intermediate type for Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDbData {
    pub id: String,
    pub status: StageStatus,
}

impl IntoDbData<AgentDbData> for Agent {
    fn into_db_data(&self) -> Result<DbData<AgentDbData>> {
        Ok(DbData {
            inner: AgentDbData {
                id: self.id.clone(),
                status: self.status.clone(),
            },
        })
    }
}
