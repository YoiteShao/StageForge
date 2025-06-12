use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use sqlx::FromRow;
use std::fmt::Debug;
use tokio::sync::mpsc;
use tracing::error;

/// Generic intermediate data type for database operations
#[derive(Debug, Clone, FromRow)]
pub struct DbData<T>
where
    T: Debug + Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    pub inner: T,
}

/// Trait for converting to intermediate database data
/// I: Input type (original data)
/// T: Target type (intermediate data)
pub trait IntoDbData<T>
where
    T: Debug + Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    /// Convert the input type into an intermediate database type
    fn into_db_data(&self) -> Result<DbData<T>>;
}

/// Operation types that can be performed on the database
#[derive(Debug)]
pub enum DbOperation<D, S>
where
    D: Debug + Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
    S: Serialize + Send + Sync + 'static,
{
    Insert {
        data: DbData<D>,
        response: tokio::sync::oneshot::Sender<Result<String>>,
    },
    Update {
        id: String,
        status: S,
        response: tokio::sync::oneshot::Sender<Result<String>>,
    },
}

/// Trait for database operations, typically implemented by specific stores
#[async_trait]
pub trait DbStore: Send + Sync + 'static {
    type Data: Debug + Send + Sync + DeserializeOwned + IntoDbData<Self::DbDataType> + 'static;
    type Status: Serialize + Send + Sync + 'static;
    type DbDataType: Debug + Clone + Send + Sync + Serialize + DeserializeOwned + 'static;

    /// Insert a new record into the store
    async fn insert(&self, data: DbData<Self::DbDataType>) -> Result<String>;

    /// Update an existing record
    async fn update(&self, id: &str, status: Self::Status) -> Result<String>;

    /// Query a record by its ID
    async fn query(&self, id: &str) -> Result<Box<dyn std::any::Any + Send>>;

    /// Process a database operation
    async fn process_operation(
        &self,
        operation: DbOperation<Self::DbDataType, Self::Status>,
    ) -> Result<()>;
}

/// Trait for database manager implementations
#[async_trait]
pub trait DatabaseManager: Send + Sync + 'static {
    type Store: DbStore;

    /// Create a new database manager instance
    async fn new(connection_string: String, max_connections: u32) -> Result<Self>
    where
        Self: Sized;

    /// Initialize the database (create tables, etc.)
    async fn init(&self) -> Result<()>;

    /// Get the store implementation
    fn get_store(&self) -> Self::Store;

    /// Get the channel sender
    fn get_sender(
        &self,
    ) -> &mpsc::Sender<
        DbOperation<<Self::Store as DbStore>::DbDataType, <Self::Store as DbStore>::Status>,
    >;

    /// Insert data through the manager
    async fn insert_data(&self, data: &<Self::Store as DbStore>::Data) -> Result<String> {
        let (sender, receiver) = tokio::sync::oneshot::channel();

        self.get_sender()
            .send(DbOperation::Insert {
                data: data.into_db_data()?,
                response: sender,
            })
            .await
            .map_err(|e| anyhow!("Failed to send insert operation: {}", e))?;

        receiver
            .await
            .map_err(|e| anyhow!("Failed to receive insert confirmation: {}", e))?
    }

    /// Update data through the manager
    async fn update_data(
        &self,
        id: String,
        status: <Self::Store as DbStore>::Status,
    ) -> Result<String> {
        let (sender, receiver) = tokio::sync::oneshot::channel();

        self.get_sender()
            .send(DbOperation::Update {
                id: id.clone(),
                status,
                response: sender,
            })
            .await
            .map_err(|e| anyhow!("Failed to send update operation: {}", e))?;

        receiver
            .await
            .map_err(|e| anyhow!("Failed to receive update confirmation: {}", e))?
    }

    /// Spawn a worker to process database operations
    fn spawn_db_worker(
        store: Self::Store,
        mut receiver: mpsc::Receiver<
            DbOperation<<Self::Store as DbStore>::DbDataType, <Self::Store as DbStore>::Status>,
        >,
    ) -> Result<()> {
        tokio::spawn(async move {
            while let Some(op) = receiver.recv().await {
                if let Err(e) = store.process_operation(op).await {
                    error!("Database operation failed: {}", e);
                }
            }
        });
        Ok(())
    }
}
