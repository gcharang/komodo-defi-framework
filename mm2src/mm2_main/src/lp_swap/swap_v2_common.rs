use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

cfg_native!(
    use crate::mm2::database::my_swaps::{get_swap_events, update_swap_events};
);

/// Represents errors that can be produced by [`MakerSwapStateMachine`] or [`TakerSwapStateMachine`] run.
#[derive(Debug, Display)]
pub enum SwapStateMachineError {
    StorageError(String),
    SerdeError(String),
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn store_swap_event<T: Serialize + DeserializeOwned>(
    ctx: &MmArc,
    id: Uuid,
    event: T,
) -> MmResult<(), SwapStateMachineError> {
    let id_str = id.to_string();
    let events_json = get_swap_events(&ctx.sqlite_connection(), &id_str)
        .map_to_mm(|e| SwapStateMachineError::StorageError(e.to_string()))?;
    let mut events: Vec<T> =
        serde_json::from_str(&events_json).map_to_mm(|e| SwapStateMachineError::SerdeError(e.to_string()))?;
    events.push(event);
    drop_mutability!(events);
    let serialized_events =
        serde_json::to_string(&events).map_to_mm(|e| SwapStateMachineError::SerdeError(e.to_string()))?;
    update_swap_events(&ctx.sqlite_connection(), &id_str, &serialized_events)
        .map_to_mm(|e| SwapStateMachineError::StorageError(e.to_string()))?;
    Ok(())
}
