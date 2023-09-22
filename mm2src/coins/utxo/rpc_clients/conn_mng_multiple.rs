use super::{ConnMngTrait, ElectrumConnSettings, ElectrumConnection};
use crate::hd_wallet::AsyncMutex;
use crate::RpcTransportEventHandlerShared;
use async_trait::async_trait;
use common::executor::abortable_queue::AbortableQueue;
use common::jsonrpc_client::JsonRpcErrorType;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct ConnMngMultiple(Arc<ConnMngMultipleImpl>);

#[derive(Debug)]
struct ConnMngMultipleImpl {
    guarded: AsyncMutex<ConnMngMultipleState>,
}

#[derive(Debug)]
struct ConnMngMultipleState {
    connections: Vec<ElectrumConnCtx>,
}

#[derive(Debug)]
struct ElectrumConnCtx {
    address: String,
    connection: Arc<AsyncMutex<ElectrumConnection>>,
}

#[async_trait]
impl ConnMngTrait for ConnMngMultiple {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>> { vec![] }

    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, String> {
        self.0.get_conn_by_address(address).await
    }

    async fn connect(&self) -> Result<(), String> { Ok(()) }

    async fn is_connected(&self) -> bool { false }

    async fn remove_server(&self, address: String) -> Result<(), String> { Ok(()) }

    async fn set_rpc_enent_handler(&self, handler: RpcTransportEventHandlerShared) {}

    async fn rotate_servers(&self, _no_of_rotations: usize) {
        // not implemented for this conn_mng implementation intentionally
    }

    async fn is_connections_pool_empty(&self) -> bool { false }

    fn on_disconnected(&self, address: String) {}
}

impl ConnMngMultipleImpl {
    pub fn new(
        servers: Vec<ElectrumConnSettings>,
        abortable_system: AbortableQueue,
        event_handlers: Vec<RpcTransportEventHandlerShared>,
    ) -> ConnMngMultipleImpl {
        ConnMngMultipleImpl {
            guarded: AsyncMutex::new(ConnMngMultipleState { connections: vec![] }),
        }
    }

    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, String> {
        let guarded = self.guarded.lock().await;
        let connection = guarded
            .connections
            .iter()
            .find(|c| c.address == address)
            .ok_or_else(|| format!("Unknown destination address {}", address))?;
        Ok(connection.connection.clone())
    }
}
