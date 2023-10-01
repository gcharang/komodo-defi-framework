use async_trait::async_trait;
use futures::lock::Mutex as AsyncMutex;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;

use super::ElectrumConnection;

/// Trait that provides a common interface to get an `ElectrumConnection` from the `ElectrumClient` instance
#[async_trait]
pub trait ConnMngTrait: Debug {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>>;
    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, String>;
    async fn connect(&self) -> Result<(), String>;
    async fn is_connected(&self) -> bool;
    async fn remove_server(&self, address: &str) -> Result<(), String>;
    async fn rotate_servers(&self, no_of_rotations: usize);
    async fn is_connections_pool_empty(&self) -> bool;
    fn on_disconnected(&self, address: &str);
}

#[async_trait]
impl ConnMngTrait for Arc<dyn ConnMngTrait + Send + Sync> {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>> { self.deref().get_conn().await }
    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, String> {
        self.deref().get_conn_by_address(address).await
    }
    async fn connect(&self) -> Result<(), String> { self.deref().connect().await }
    async fn is_connected(&self) -> bool { self.deref().is_connected().await }
    async fn remove_server(&self, address: &str) -> Result<(), String> { self.deref().remove_server(address).await }
    async fn rotate_servers(&self, no_of_rotations: usize) { self.deref().rotate_servers(no_of_rotations).await }
    async fn is_connections_pool_empty(&self) -> bool { self.deref().is_connections_pool_empty().await }
    fn on_disconnected(&self, address: &str) { self.deref().on_disconnected(address) }
}
