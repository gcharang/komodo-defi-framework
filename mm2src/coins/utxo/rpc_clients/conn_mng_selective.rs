use async_trait::async_trait;
use futures::future::FutureExt;
use futures::lock::{Mutex as AsyncMutex, MutexGuard};
use futures::select;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Duration;

use common::executor::abortable_queue::{AbortableQueue, WeakSpawner};
use common::executor::{AbortableSystem, SpawnFuture, Timer};
use common::log::{debug, error, info, warn};
use mm2_rpc::data::legacy::Priority;

use super::conn_mng_common::{ConnMngError, ConnMngTrait, DEFAULT_CONN_TIMEOUT_SEC, SUSPEND_TIMEOUT_INIT_SEC};
use super::{spawn_electrum, ElectrumClientEvent, ElectrumConnSettings, ElectrumConnection};

#[derive(Debug)]
pub struct ConnMngSelectiveImpl {
    guarded: AsyncMutex<ConnMngSelectiveState>,
    abortable_system: AbortableQueue,
    event_sender: futures::channel::mpsc::UnboundedSender<ElectrumClientEvent>,
}

#[derive(Debug)]
struct ConnMngSelectiveState {
    connecting: AtomicBool,
    queue: ConnMngSelectiveQueue,
    active: Option<String>,
    conn_ctxs: BTreeMap<String, ElectrumConnCtx>,
}

#[derive(Debug)]
struct ElectrumConnCtx {
    conn_settings: ElectrumConnSettings,
    abortable_system: AbortableQueue,
    suspend_timeout_sec: u64,
    connection: Option<Arc<AsyncMutex<ElectrumConnection>>>,
}

#[derive(Debug)]
struct ConnMngSelectiveQueue {
    primary: VecDeque<String>,
    backup: VecDeque<String>,
}

#[derive(Clone, Debug)]
pub struct ConnMngSelective(pub Arc<ConnMngSelectiveImpl>);

struct ConnectingAtomicCtx {
    conn_mng: ConnMngSelective,
    mng_spawner: WeakSpawner,
}

#[async_trait]
impl ConnMngTrait for ConnMngSelective {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>> { self.0.get_conn().await }

    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, ConnMngError> {
        self.0.get_conn_by_address(address).await
    }

    async fn connect(&self) -> Result<(), ConnMngError> {
        while let Some((conn_settings, weak_spawner, _connecting_state_ctx)) = self.fetch_conn_settings().await {
            debug!("Got conn_settings to connect to: {:?}", conn_settings);
            let address = conn_settings.url.clone();
            match self
                .connect_to(conn_settings, weak_spawner, self.0.event_sender.clone())
                .await
            {
                Ok(_) => {
                    ConnMngSelectiveImpl::set_active_conn(&mut self.0.guarded.lock().await, address)?;
                    break;
                },
                Err(_) => {
                    self.clone().suspend_server(address.clone()).await?;
                },
            };
        }
        Ok(())
    }

    async fn is_connected(&self) -> bool { self.0.is_connected().await }

    async fn remove_server(&self, address: &str) -> Result<(), ConnMngError> { self.0.remove_server(address).await }

    async fn rotate_servers(&self, _no_of_rotations: usize) {
        // not implemented for the conn mng selective
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
            if let Err(err) = self_copy.connect().await {
                error!(
                    "Failed to reconnect after addr was disconnected: {}, error: {}",
                    address, err
                );
            }
        });
    }
}

impl ConnMngSelective {
    async fn fetch_conn_settings(&self) -> Option<(ElectrumConnSettings, WeakSpawner, ConnectingAtomicCtx)> {
        let mut guard = self.0.guarded.lock().await;
        if guard.active.is_some() {
            warn!("Skip connecting, already connected");
            return None;
        }

        let mng_spawner = self.0.abortable_system.weak_spawner();
        let Some(connecting_state_ctx) = ConnectingAtomicCtx::try_new(& mut guard, self.clone(), mng_spawner) else {
            warn!("Skip connecting, is in progress");
            return None
        };

        debug!("Primary electrum nodes to connect: {:?}", guard.queue.primary);
        debug!("Backup electrum nodes to connect: {:?}", guard.queue.backup);
        let mut iter = guard.queue.primary.iter().chain(guard.queue.backup.iter());
        let addr = iter.next()?.clone();
        if let Ok(conn_ctx) = Self::get_conn_ctx(&guard, &addr) {
            Some((
                conn_ctx.conn_settings.clone(),
                conn_ctx.abortable_system.weak_spawner(),
                connecting_state_ctx,
            ))
        } else {
            warn!("Failed to connect, no connection settings found");
            None
        }
    }

    async fn suspend_server(&self, address: String) -> Result<(), ConnMngError> {
        debug!(
            "About to suspend connection to addr: {}, guard: {:?}",
            address, self.0.guarded
        );
        let mut guard = self.0.guarded.lock().await;
        if let Some(ref active) = guard.active {
            if *active == address {
                guard.active.take();
            }
        }

        match Self::get_conn_ctx(&guard, &address)?.conn_settings.priority {
            Priority::Primary => {
                guard.queue.primary.pop_front();
            },
            Priority::Secondary => {
                guard.queue.backup.pop_front();
            },
        };

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

    async fn resume_server(self, address: String) -> Result<(), ConnMngError> {
        debug!("Resume address: {}", address);
        let mut guard = self.0.guarded.lock().await;
        let priority = Self::get_conn_ctx(&guard, &address)?.conn_settings.priority.clone();
        match priority {
            Priority::Primary => guard.queue.primary.push_back(address.clone()),
            Priority::Secondary => guard.queue.backup.push_back(address.clone()),
        }

        if let Some(active) = guard.active.clone() {
            let conn_ctx = Self::get_conn_ctx(&guard, &address)?;
            let active_ctx = Self::get_conn_ctx(&guard, &active)?;
            let active_priority = &active_ctx.conn_settings.priority;
            if let (Priority::Secondary, Priority::Primary) = (active_priority, priority) {
                let conn_settings = conn_ctx.conn_settings.clone();
                let conn_spawner = conn_ctx.abortable_system.weak_spawner();
                drop(guard);
                if let Err(err) = self
                    .clone()
                    .connect_to(conn_settings, conn_spawner, self.0.event_sender.clone())
                    .await
                {
                    error!("Failed to resume: {}", err);
                    self.suspend_server(address.clone()).await?;
                } else {
                    let mut guard = self.0.guarded.lock().await;
                    Self::reset_connection_context(
                        &mut guard,
                        &active,
                        self.0.abortable_system.create_subsystem().unwrap(),
                    )?;
                    ConnMngSelectiveImpl::set_active_conn(&mut guard, address.clone())?;
                }
            }
        } else {
            drop(guard);
            let _ = self.connect().await;
        };
        Ok(())
    }

    fn reset_connection_context(
        state: &mut MutexGuard<'_, ConnMngSelectiveState>,
        address: &str,
        abortable_system: AbortableQueue,
    ) -> Result<(), ConnMngError> {
        debug!("Reset connection context for: {}", address);

        let conn_ctx = Self::get_conn_ctx_mut(state, address)?;
        conn_ctx
            .abortable_system
            .abort_all()
            .map_err(|err| ConnMngError::FailedAbort(address.to_string(), err))?;
        conn_ctx.connection.take();
        conn_ctx.abortable_system = abortable_system;
        Ok(())
    }

    fn register_connection(
        state: &mut MutexGuard<'_, ConnMngSelectiveState>,
        conn: ElectrumConnection,
    ) -> Result<(), ConnMngError> {
        let conn_ctx = Self::get_conn_ctx_mut(state, &conn.addr)?;
        conn_ctx.connection.replace(Arc::new(AsyncMutex::new(conn)));
        Ok(())
    }

    async fn get_suspend_timeout(
        state: &MutexGuard<'_, ConnMngSelectiveState>,
        address: &str,
    ) -> Result<u64, ConnMngError> {
        Self::get_conn_ctx(state, address).map(|ctx| ctx.suspend_timeout_sec)
    }

    fn duplicate_suspend_timeout(
        state: &mut MutexGuard<'_, ConnMngSelectiveState>,
        address: &str,
    ) -> Result<(), ConnMngError> {
        Self::set_suspend_timeout(state, address, |origin| origin.checked_mul(2).unwrap_or(u64::MAX))
    }

    fn reset_suspend_timeout(
        state: &mut MutexGuard<'_, ConnMngSelectiveState>,
        address: &str,
    ) -> Result<(), ConnMngError> {
        Self::set_suspend_timeout(state, address, |_| SUSPEND_TIMEOUT_INIT_SEC)
    }

    fn set_suspend_timeout<F: Fn(u64) -> u64>(
        state: &mut MutexGuard<'_, ConnMngSelectiveState>,
        address: &str,
        method: F,
    ) -> Result<(), ConnMngError> {
        let conn_ctx = Self::get_conn_ctx_mut(state, address)?;
        let suspend_timeout = &mut conn_ctx.suspend_timeout_sec;
        let new_value = method(*suspend_timeout);
        debug!(
            "Set supsend timeout for address: {} - from: {} to the value: {}",
            address, suspend_timeout, new_value
        );
        *suspend_timeout = new_value;
        Ok(())
    }

    async fn connect_to(
        &self,
        conn_settings: ElectrumConnSettings,
        weak_spawner: WeakSpawner,
        event_sender: futures::channel::mpsc::UnboundedSender<ElectrumClientEvent>,
    ) -> Result<(), ConnMngError> {
        let (conn, mut conn_ready_receiver) = spawn_electrum(&conn_settings, weak_spawner.clone(), event_sender)?;
        Self::register_connection(&mut self.0.guarded.lock().await, conn)?;
        let timeout_sec = conn_settings.timeout_sec.unwrap_or(DEFAULT_CONN_TIMEOUT_SEC);

        select! {
            _ = async_std::task::sleep(Duration::from_secs(timeout_sec)).fuse() => {
                warn!("Failed to connect to: {}, timed out", conn_settings.url);
                Err(ConnMngError::ConnectingError(conn_settings.url.clone(), format!("Timed out: {}", timeout_sec)))
            },
            _ = conn_ready_receiver => Ok(()) // TODO: handle cancelled
        }
    }

    fn get_conn_ctx<'a>(
        state: &'a MutexGuard<'a, ConnMngSelectiveState>,
        address: &str,
    ) -> Result<&'a ElectrumConnCtx, ConnMngError> {
        state
            .conn_ctxs
            .get(address)
            .ok_or_else(|| ConnMngError::UnknownAddress(address.to_string()))
    }

    fn get_conn_ctx_mut<'a, 'b>(
        state: &'a mut MutexGuard<'b, ConnMngSelectiveState>,
        address: &'_ str,
    ) -> Result<&'a mut ElectrumConnCtx, ConnMngError> {
        state
            .conn_ctxs
            .get_mut(address)
            .ok_or_else(|| ConnMngError::UnknownAddress(address.to_string()))
    }
}

impl ConnMngSelectiveImpl {
    pub(super) fn try_new(
        servers: Vec<ElectrumConnSettings>,
        abortable_system: AbortableQueue,
        event_sender: futures::channel::mpsc::UnboundedSender<ElectrumClientEvent>,
    ) -> Result<ConnMngSelectiveImpl, String> {
        let mut primary = VecDeque::<String>::new();
        let mut backup = VecDeque::<String>::new();
        let mut conn_ctxs: BTreeMap<String, ElectrumConnCtx> = BTreeMap::new();
        for conn_settings in servers {
            match conn_settings.priority {
                Priority::Primary => primary.push_back(conn_settings.url.clone()),
                Priority::Secondary => backup.push_back(conn_settings.url.clone()),
            }
            let conn_abortable_system = abortable_system.create_subsystem().map_err(|err| {
                ERRL!(
                    "Failed to create abortable subsystem for conn: {}, error: {}",
                    conn_settings.url,
                    err
                )
            })?;
            let _ = conn_ctxs.insert(conn_settings.url.clone(), ElectrumConnCtx {
                conn_settings,
                connection: None,
                abortable_system: conn_abortable_system,
                suspend_timeout_sec: SUSPEND_TIMEOUT_INIT_SEC,
            });
        }

        Ok(ConnMngSelectiveImpl {
            event_sender,
            guarded: AsyncMutex::new(ConnMngSelectiveState {
                connecting: AtomicBool::new(false),
                queue: ConnMngSelectiveQueue { primary, backup },
                active: None,
                conn_ctxs,
            }),
            abortable_system,
        })
    }

    async fn remove_server(&self, address: &str) -> Result<(), ConnMngError> {
        debug!("Remove server: {}", address);
        let mut guard = self.guarded.lock().await;
        let conn_ctx = guard
            .conn_ctxs
            .remove(address)
            .ok_or_else(|| ConnMngError::UnknownAddress(address.to_string()))?;

        match conn_ctx.conn_settings.priority {
            Priority::Primary => guard.queue.primary.pop_front(),
            Priority::Secondary => guard.queue.backup.pop_front(),
        };
        if let Some(active) = guard.active.as_ref() {
            if active == address {
                guard.active.take();
            }
        }
        Ok(())
    }

    fn set_active_conn(guard: &mut MutexGuard<'_, ConnMngSelectiveState>, address: String) -> Result<(), ConnMngError> {
        ConnMngSelective::reset_suspend_timeout(guard, &address)?;
        let _ = guard.active.replace(address);
        Ok(())
    }

    async fn is_connected(&self) -> bool { self.guarded.lock().await.active.is_some() }

    async fn is_connections_pool_empty(&self) -> bool { self.guarded.lock().await.conn_ctxs.is_empty() }

    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>> {
        debug!("Getting available connection");
        let guard = self.guarded.lock().await;
        let Some(address) = guard.active.as_ref().cloned() else {
            return vec![];
        };

        let conn_ctx = match ConnMngSelective::get_conn_ctx(&guard, &address) {
            Ok(conn_ctx) => conn_ctx,
            Err(err) => {
                error!("{}", err);
                return vec![];
            },
        };

        if let Some(conn) = conn_ctx.connection.clone() {
            vec![conn]
        } else {
            vec![]
        }
    }

    async fn get_conn_by_address(&self, address: &str) -> Result<Arc<AsyncMutex<ElectrumConnection>>, ConnMngError> {
        debug!("Getting connection for address: {:?}", address);
        let guard = self.guarded.lock().await;

        let conn_ctx = ConnMngSelective::get_conn_ctx(&guard, address)?;
        conn_ctx
            .connection
            .clone()
            .ok_or_else(|| ConnMngError::NotConnected(address.to_string()))
    }
}

impl ConnectingAtomicCtx {
    fn try_new(
        guarded: &mut MutexGuard<'_, ConnMngSelectiveState>,
        conn_mng: ConnMngSelective,
        mng_spawner: WeakSpawner,
    ) -> Option<ConnectingAtomicCtx> {
        match guarded
            .connecting
            .compare_exchange(false, true, AtomicOrdering::Acquire, AtomicOrdering::Relaxed)
        {
            Ok(false) => Some(Self { conn_mng, mng_spawner }),
            Err(true) => None,
            _ => panic!("Failed to connect: unexpected state on compare_exchange connecting state"),
        }
    }
}

impl Drop for ConnectingAtomicCtx {
    fn drop(&mut self) {
        let spawner = self.mng_spawner.clone();
        let conn_mng = self.conn_mng.clone();
        spawner.spawn(async move {
            let state = conn_mng.0.guarded.lock().await;
            state.connecting.store(false, AtomicOrdering::Relaxed);
        })
    }
}
