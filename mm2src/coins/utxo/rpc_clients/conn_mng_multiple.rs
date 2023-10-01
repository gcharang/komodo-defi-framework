use async_trait::async_trait;
use core::time::Duration;
use futures::lock::{Mutex as AsyncMutex, MutexGuard};
use futures::{select, FutureExt};
use std::ops::Deref;
use std::sync::Arc;

use common::executor::abortable_queue::{AbortableQueue, WeakSpawner};
use common::executor::{AbortableSystem, SpawnFuture, Timer};
use common::log::{debug, error, info, warn};

use super::conn_mng_common::ConnMngTrait;
use super::{ElectrumConnSettings, ElectrumConnection, DEFAULT_CONN_TIMEOUT_SEC, SUSPEND_TIMEOUT_INIT_SEC};
use crate::utxo::rpc_clients::{spawn_electrum, ElectrumClientEvent};

#[derive(Clone, Debug)]
pub struct ConnMngMultiple(pub Arc<ConnMngMultipleImpl>);

#[derive(Debug)]
pub struct ConnMngMultipleImpl {
    guarded: AsyncMutex<ConnMngMultipleState>,
    abortable_system: AbortableQueue,
    event_sender: futures::channel::mpsc::UnboundedSender<ElectrumClientEvent>,
}

#[derive(Debug)]
struct ConnMngMultipleState {
    conn_ctxs: Vec<ElectrumConnCtx>,
}

#[derive(Debug)]
struct ElectrumConnCtx {
    conn_settings: ElectrumConnSettings,
    abortable_system: AbortableQueue,
    suspend_timeout_sec: u64,
    connection: Option<Arc<AsyncMutex<ElectrumConnection>>>,
}

#[async_trait]
impl ConnMngTrait for ConnMngMultiple {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>> { self.0.get_conn().await }

    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, String> {
        self.0.get_conn_by_address(address).await
    }

    async fn connect(&self) -> Result<(), String> { self.deref().connect().await }

    async fn is_connected(&self) -> bool { self.0.is_connected().await }

    async fn remove_server(&self, address: &str) -> Result<(), String> { self.0.remove_server(address).await }

    async fn rotate_servers(&self, no_of_rotations: usize) {
        debug!("Rotate servers: {}", no_of_rotations);
        let mut guarded = self.0.guarded.lock().await;
        guarded.conn_ctxs.rotate_left(no_of_rotations);
    }

    async fn is_connections_pool_empty(&self) -> bool { self.0.is_connections_pool_empty().await }

    fn on_disconnected(&self, address: &str) {
        info!(
            "electrum_conn_mng disconnected from: {}, it will be suspended and trying to reconnect",
            address
        );
        let self_copy = self.clone();
        let address = address.to_string();
        self.0.abortable_system.weak_spawner().spawn(async move {
            if let Err(err) = self_copy.clone().suspend_server(address.clone()).await {
                error!("Failed to suspend server: {}, error: {}", address, err);
            }
        });
    }
}

impl ConnMngMultiple {
    async fn connect(&self) -> Result<(), String> {
        let mut guarded = self.0.guarded.lock().await;

        if guarded.conn_ctxs.is_empty() {
            return ERR!("Not settings to connect to found");
        }

        for conn_ctx in &mut guarded.conn_ctxs {
            if conn_ctx.connection.is_some() {
                let address = &conn_ctx.conn_settings.url;
                warn!("An attempt to connect over an existing one: {}", address);
                continue;
            }
            let conn_settings = conn_ctx.conn_settings.clone();
            let weak_spawner = conn_ctx.abortable_system.weak_spawner();
            let self_clone = self.clone();
            self.0.abortable_system.weak_spawner().spawn(async move {
                let _ = self_clone.connect_to(conn_settings, weak_spawner).await;
            });
        }
        Ok(())
    }

    async fn suspend_server(&self, address: String) -> Result<(), String> {
        debug!(
            "About to suspend connection to addr: {}, guard: {:?}",
            address, self.0.guarded
        );
        let mut guard = self.0.guarded.lock().await;

        Self::reset_connection_context(
            &mut guard,
            &address,
            self.0.abortable_system.create_subsystem().unwrap(),
        )?;

        let suspend_timeout_sec = Self::get_suspend_timeout(&guard, &address).await?;
        Self::duplicate_suspend_timeout(&mut guard, &address)?;
        drop(guard);

        self.clone().spawn_resume_server(address, suspend_timeout_sec);
        debug!("Suspend future spawned");
        Ok(())
    }

    // workaround to avoid the cycle detected compilation error that blocks recursive async calls
    fn spawn_resume_server(self, address: String, suspend_timeout_sec: u64) {
        let spawner = self.0.abortable_system.weak_spawner();
        spawner.spawn(Box::new(
            async move {
                debug!("Suspend server: {}, for: {} seconds", address, suspend_timeout_sec);
                Timer::sleep(suspend_timeout_sec as f64).await;
                let _ = self.resume_server(address).await;
            }
            .boxed(),
        ));
    }

    async fn resume_server(self, address: String) -> Result<(), String> {
        debug!("Resume address: {}", address);
        let guard = self.0.guarded.lock().await;

        let (_, conn_ctx) = Self::get_conn_ctx(&guard, &address)?;
        let conn_settings = conn_ctx.conn_settings.clone();
        let conn_spawner = conn_ctx.abortable_system.weak_spawner();
        drop(guard);

        if let Err(err) = self.clone().connect_to(conn_settings, conn_spawner).await {
            error!("Failed to resume: {}", err);
            self.suspend_server(address.clone())
                .await
                .map_err(|err| ERRL!("Failed to suspend server: {}, error: {}", address, err))?;
        }
        Ok(())
    }

    fn reset_connection_context(
        state: &mut MutexGuard<'_, ConnMngMultipleState>,
        address: &str,
        abortable_system: AbortableQueue,
    ) -> Result<(), String> {
        debug!("Reset connection context for: {}", address);
        let (_, conn_ctx) = Self::get_conn_ctx_mut(state, address)?;
        conn_ctx.abortable_system.abort_all().map_err(|err| {
            ERRL!(
                "Failed to abort on electrum connection related spawner: {}, error: {:?}",
                address,
                err
            )
        })?;
        conn_ctx.connection.take();
        conn_ctx.abortable_system = abortable_system;
        Ok(())
    }

    async fn get_suspend_timeout(state: &MutexGuard<'_, ConnMngMultipleState>, address: &str) -> Result<u64, String> {
        Self::get_conn_ctx(state, address).map(|(_, conn_ctx)| conn_ctx.suspend_timeout_sec)
    }

    fn duplicate_suspend_timeout(
        state: &mut MutexGuard<'_, ConnMngMultipleState>,
        address: &str,
    ) -> Result<(), String> {
        Self::set_suspend_timeout(state, address, |origin| origin.checked_mul(2).unwrap_or(u64::MAX))
    }

    fn reset_suspend_timeout(state: &mut MutexGuard<'_, ConnMngMultipleState>, address: &str) -> Result<(), String> {
        Self::set_suspend_timeout(state, address, |_| SUSPEND_TIMEOUT_INIT_SEC)
    }

    fn set_suspend_timeout<F: Fn(u64) -> u64>(
        state: &mut MutexGuard<'_, ConnMngMultipleState>,
        address: &str,
        method: F,
    ) -> Result<(), String> {
        let conn_ctx = Self::get_conn_ctx_mut(state, address)?;
        let suspend_timeout = &mut conn_ctx.1.suspend_timeout_sec;
        let new_value = method(*suspend_timeout);
        debug!(
            "Set supsend timeout for address: {} - from: {} to the value: {}",
            address, suspend_timeout, new_value
        );
        *suspend_timeout = new_value;
        Ok(())
    }

    fn get_conn_ctx<'a>(
        state: &'a MutexGuard<'a, ConnMngMultipleState>,
        address: &str,
    ) -> Result<(usize, &'a ElectrumConnCtx), String> {
        state
            .conn_ctxs
            .iter()
            .enumerate()
            .find(|(_, c)| c.conn_settings.url == address)
            .ok_or_else(|| format!("Unknown address {}", address))
    }

    fn get_conn_ctx_mut<'a, 'b>(
        state: &'a mut MutexGuard<'b, ConnMngMultipleState>,
        address: &'_ str,
    ) -> Result<(usize, &'a mut ElectrumConnCtx), String> {
        state
            .conn_ctxs
            .iter_mut()
            .enumerate()
            .find(|(_, ctx)| ctx.conn_settings.url == address)
            .ok_or_else(|| format!("Unknown address {}", address))
    }

    async fn connect_to(self, conn_settings: ElectrumConnSettings, weak_spawner: WeakSpawner) -> Result<(), String> {
        let (conn, mut conn_ready_receiver) =
            spawn_electrum(&conn_settings, weak_spawner.clone(), self.0.event_sender.clone())?;
        Self::register_connection(&mut self.0.guarded.lock().await, conn)?;
        let timeout_sec = conn_settings.timeout_sec.unwrap_or(DEFAULT_CONN_TIMEOUT_SEC);
        let address = conn_settings.url.clone();
        select! {
            _ = async_std::task::sleep(Duration::from_secs(timeout_sec)).fuse() => {
                self
                .suspend_server(address.clone())
                .await
                .map_err(|err| ERRL!("Failed to suspend server: {}, error: {}", address, err))
            },
            _ = conn_ready_receiver => {
                ConnMngMultiple::reset_suspend_timeout(&mut self.0.guarded.lock().await, &address)?;
                Ok(())
            }
        }
    }

    fn register_connection(
        state: &mut MutexGuard<'_, ConnMngMultipleState>,
        conn: ElectrumConnection,
    ) -> Result<(), String> {
        let (_, conn_ctx) = Self::get_conn_ctx_mut(state, &conn.addr)?;
        conn_ctx.connection.replace(Arc::new(AsyncMutex::new(conn)));
        Ok(())
    }
}

impl ConnMngMultipleImpl {
    pub(super) fn new(
        servers: Vec<ElectrumConnSettings>,
        abortable_system: AbortableQueue,
        event_sender: futures::channel::mpsc::UnboundedSender<ElectrumClientEvent>,
    ) -> ConnMngMultipleImpl {
        let mut connections: Vec<ElectrumConnCtx> = vec![];
        for conn_settings in servers {
            let subsystem: AbortableQueue = abortable_system.create_subsystem().unwrap();

            connections.push(ElectrumConnCtx {
                conn_settings,
                abortable_system: subsystem,
                suspend_timeout_sec: SUSPEND_TIMEOUT_INIT_SEC,
                connection: None,
            });
        }

        ConnMngMultipleImpl {
            abortable_system,
            event_sender,
            guarded: AsyncMutex::new(ConnMngMultipleState { conn_ctxs: connections }),
        }
    }

    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>> {
        let connections = &self.guarded.lock().await.conn_ctxs;
        connections
            .iter()
            .filter(|conn_ctx| conn_ctx.connection.is_some())
            .map(|conn_ctx| conn_ctx.connection.as_ref().unwrap().clone())
            .collect()
    }

    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, String> {
        let guarded = self.guarded.lock().await;
        let (_, conn_ctx) = ConnMngMultiple::get_conn_ctx(&guarded, address)?;
        conn_ctx
            .connection
            .as_ref()
            .cloned()
            .ok_or_else(|| format!("Connection is not established for address {}", address))
    }

    async fn is_connected(&self) -> bool {
        let guarded = self.guarded.lock().await;

        for conn_ctx in guarded.conn_ctxs.iter() {
            if let Some(ref connection) = conn_ctx.connection {
                if connection.lock().await.is_connected().await {
                    return true;
                }
            }
        }

        false
    }

    async fn remove_server(&self, address: &str) -> Result<(), String> {
        debug!("Remove electrum server: {}", address);
        let mut guarded = self.guarded.lock().await;
        let (i, _) = ConnMngMultiple::get_conn_ctx(&guarded, address)?;
        let conn_ctx = guarded.conn_ctxs.remove(i);
        conn_ctx.abortable_system.abort_all().map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn is_connections_pool_empty(&self) -> bool { self.guarded.lock().await.conn_ctxs.is_empty() }
}
