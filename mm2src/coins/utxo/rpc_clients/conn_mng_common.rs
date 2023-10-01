use async_trait::async_trait;
use common::executor::AbortedError;
use derive_more::Display;
use futures::lock::Mutex as AsyncMutex;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;

use super::ElectrumConnection;

/// Trait that provides a common interface to get an `ElectrumConnection` from the `ElectrumClient` instance
#[async_trait]
pub(super) trait ConnMngTrait: Debug {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>>;
    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, ConnMngError>;
    async fn connect(&self) -> Result<(), ConnMngError>;
    async fn is_connected(&self) -> bool;
    async fn remove_server(&self, address: &str) -> Result<(), ConnMngError>;
    async fn rotate_servers(&self, no_of_rotations: usize);
    async fn is_connections_pool_empty(&self) -> bool;
    fn on_disconnected(&self, address: &str);
}

#[async_trait]
impl ConnMngTrait for Arc<dyn ConnMngTrait + Send + Sync> {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>> { self.deref().get_conn().await }
    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, ConnMngError> {
        self.deref().get_conn_by_address(address).await
    }
    async fn connect(&self) -> Result<(), ConnMngError> { self.deref().connect().await }
    async fn is_connected(&self) -> bool { self.deref().is_connected().await }
    async fn remove_server(&self, address: &str) -> Result<(), ConnMngError> {
        self.deref().remove_server(address).await
    }
    async fn rotate_servers(&self, no_of_rotations: usize) { self.deref().rotate_servers(no_of_rotations).await }
    async fn is_connections_pool_empty(&self) -> bool { self.deref().is_connections_pool_empty().await }
    fn on_disconnected(&self, address: &str) { self.deref().on_disconnected(address) }
}

#[derive(Debug, Display)]
pub(super) enum ConnMngError {
    #[display(fmt = "Unknown address: {}", _0)]
    UnknownAddress(String),
    #[display(fmt = "Connection is not established, {}", _0)]
    NotConnected(String),
    #[display(fmt = "Failed to abort abortable system for: {}, error: {}", _0, _1)]
    FailedAbort(String, AbortedError),
    #[display(fmt = "Failed to connect to: {}, error: {}", _0, _1)]
    ConnectingError(String, String),
    #[display(fmt = "No settings to connect to found")]
    SettingsNotSet,
}
