pub mod agent;
pub mod db;
pub mod error;
pub mod pipeline;
pub mod stage;
pub mod utils;

// Re-export main types
pub use agent::{Agent, AgentStage, StageStatus};
pub use db::DatabaseManager;
pub use pipeline::Pipeline;
pub use stage::Stage;

/// Error type for the multi-pipeline library
pub type Result<T> = std::result::Result<T, error::Error>;
