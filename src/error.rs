use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Stage error: {0}")]
    Stage(String),

    #[error("Pipeline error: {0}")]
    Pipeline(String),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Stage {0} already registered")]
    StageAlreadyRegistered(String),

    #[error("Stage {0} not found")]
    StageNotFound(String),

    #[error("Agent {0} not found")]
    AgentNotFound(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::Database(err.to_string())
    }
}
