use crate::mm2::lp_network::subscribe_to_topic;
use crate::mm2::lp_swap::SwapsContext;
use common::bits256;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use mm2_state_machine::storable_state_machine::StateMachineDbRepr;
use rpc::v1::types::Bytes as BytesJson;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Error;
use uuid::Uuid;

cfg_native!(
    use common::async_blocking;
    use crate::mm2::database::my_swaps::{does_swap_exist, get_swap_events, update_swap_events,
                                     select_unfinished_swaps_uuids, set_swap_is_finished};
);

cfg_wasm32!(
    use common::bool_as_int::BoolAsInt;
    use crate::mm2::lp_swap::swap_wasm_db::{IS_FINISHED_SWAP_TYPE_INDEX, MySwapsFiltersTable, SavedSwapTable};
    use mm2_db::indexed_db::{DbTransactionError, InitDbError, MultiIndex};
);

/// DB representation of tx preimage with signature
#[derive(Debug, Deserialize, Serialize)]
pub struct StoredTxPreimage {
    pub preimage: BytesJson,
    pub signature: BytesJson,
}

/// Represents error variants, which can happen on swaps re-creation
#[derive(Debug, Display)]
pub enum SwapRecreateError {
    /// DB representation has empty events
    ReprEventsEmpty,
    /// Failed to parse some data from DB representation (e.g. transactions, pubkeys, etc.)
    FailedToParseData(String),
    /// Swap has been aborted
    SwapAborted,
    /// Swap has been completed
    SwapCompleted,
}

/// Represents errors that can be produced by [`MakerSwapStateMachine`] or [`TakerSwapStateMachine`] run.
#[derive(Debug, Display)]
pub enum SwapStateMachineError {
    StorageError(String),
    SerdeError(String),
    #[cfg(target_arch = "wasm32")]
    NoSwapWithUuid(Uuid),
}

pub struct SwapRecreateCtx<MakerCoin, TakerCoin> {
    pub maker_coin: MakerCoin,
    pub taker_coin: TakerCoin,
}

#[cfg(not(target_arch = "wasm32"))]
impl From<db_common::sqlite::rusqlite::Error> for SwapStateMachineError {
    fn from(e: db_common::sqlite::rusqlite::Error) -> Self { SwapStateMachineError::StorageError(e.to_string()) }
}

impl From<serde_json::Error> for SwapStateMachineError {
    fn from(e: Error) -> Self { SwapStateMachineError::SerdeError(e.to_string()) }
}

#[cfg(target_arch = "wasm32")]
impl From<InitDbError> for SwapStateMachineError {
    fn from(e: InitDbError) -> Self { SwapStateMachineError::StorageError(e.to_string()) }
}

#[cfg(target_arch = "wasm32")]
impl From<DbTransactionError> for SwapStateMachineError {
    fn from(e: DbTransactionError) -> Self { SwapStateMachineError::StorageError(e.to_string()) }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn has_db_record_for(ctx: MmArc, id: &Uuid) -> MmResult<bool, SwapStateMachineError> {
    let id_str = id.to_string();
    Ok(async_blocking(move || does_swap_exist(&ctx.sqlite_connection(), &id_str)).await?)
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn has_db_record_for(ctx: MmArc, id: &Uuid) -> MmResult<bool, SwapStateMachineError> {
    let swaps_ctx = SwapsContext::from_ctx(&ctx).expect("SwapsContext::from_ctx should not fail");
    let db = swaps_ctx.swap_db().await?;
    let transaction = db.transaction().await?;
    let table = transaction.table::<MySwapsFiltersTable>().await?;
    let maybe_item = table.get_item_by_unique_index("uuid", id).await?;
    Ok(maybe_item.is_some())
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn store_swap_event<T: StateMachineDbRepr>(
    ctx: MmArc,
    id: Uuid,
    event: T::Event,
) -> MmResult<(), SwapStateMachineError>
where
    T::Event: DeserializeOwned + Serialize + Send + 'static,
{
    let id_str = id.to_string();
    async_blocking(move || {
        let events_json = get_swap_events(&ctx.sqlite_connection(), &id_str)?;
        let mut events: Vec<T::Event> = serde_json::from_str(&events_json)?;
        events.push(event);
        drop_mutability!(events);
        let serialized_events = serde_json::to_string(&events)?;
        update_swap_events(&ctx.sqlite_connection(), &id_str, &serialized_events)?;
        Ok(())
    })
    .await
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn store_swap_event<T: StateMachineDbRepr + DeserializeOwned + Serialize + Send + 'static>(
    ctx: MmArc,
    id: Uuid,
    event: T::Event,
) -> MmResult<(), SwapStateMachineError> {
    let swaps_ctx = SwapsContext::from_ctx(&ctx).expect("SwapsContext::from_ctx should not fail");
    let db = swaps_ctx.swap_db().await?;
    let transaction = db.transaction().await?;
    let table = transaction.table::<SavedSwapTable>().await?;

    let saved_swap_json = match table.get_item_by_unique_index("uuid", id).await? {
        Some((_item_id, SavedSwapTable { saved_swap, .. })) => saved_swap,
        None => return MmError::err(SwapStateMachineError::NoSwapWithUuid(id)),
    };

    let mut swap_repr: T = serde_json::from_value(saved_swap_json)?;
    swap_repr.add_event(event);

    let new_item = SavedSwapTable {
        uuid: id,
        saved_swap: serde_json::to_value(swap_repr)?,
    };
    table.replace_item_by_unique_index("uuid", id, &new_item).await?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn get_swap_repr<T: DeserializeOwned>(ctx: &MmArc, id: Uuid) -> MmResult<T, SwapStateMachineError> {
    let swaps_ctx = SwapsContext::from_ctx(&ctx).expect("SwapsContext::from_ctx should not fail");
    let db = swaps_ctx.swap_db().await?;
    let transaction = db.transaction().await?;

    let table = transaction.table::<SavedSwapTable>().await?;
    let saved_swap_json = match table.get_item_by_unique_index("uuid", id).await? {
        Some((_item_id, SavedSwapTable { saved_swap, .. })) => saved_swap,
        None => return MmError::err(SwapStateMachineError::NoSwapWithUuid(id)),
    };

    let swap_repr = serde_json::from_value(saved_swap_json)?;
    Ok(swap_repr)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn get_unfinished_swaps_uuids(
    ctx: MmArc,
    swap_type: u8,
) -> MmResult<Vec<Uuid>, SwapStateMachineError> {
    async_blocking(move || {
        select_unfinished_swaps_uuids(&ctx.sqlite_connection(), swap_type)
            .map_to_mm(|e| SwapStateMachineError::StorageError(e.to_string()))
    })
    .await
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn get_unfinished_swaps_uuids(
    ctx: MmArc,
    swap_type: u8,
) -> MmResult<Vec<Uuid>, SwapStateMachineError> {
    let index = MultiIndex::new(IS_FINISHED_SWAP_TYPE_INDEX)
        .with_value(BoolAsInt::new(false))?
        .with_value(swap_type)?;

    let swaps_ctx = SwapsContext::from_ctx(&ctx).expect("SwapsContext::from_ctx should not fail");
    let db = swaps_ctx.swap_db().await?;
    let transaction = db.transaction().await?;
    let table = transaction.table::<MySwapsFiltersTable>().await?;
    let table_items = table.get_items_by_multi_index(index).await?;

    Ok(table_items.into_iter().map(|(_item_id, item)| item.uuid).collect())
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn mark_swap_finished(ctx: MmArc, id: Uuid) -> MmResult<(), SwapStateMachineError> {
    async_blocking(move || Ok(set_swap_is_finished(&ctx.sqlite_connection(), &id.to_string())?)).await
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn mark_swap_finished(ctx: MmArc, id: Uuid) -> MmResult<(), SwapStateMachineError> {
    let swaps_ctx = SwapsContext::from_ctx(&ctx).expect("SwapsContext::from_ctx should not fail");
    let db = swaps_ctx.swap_db().await?;
    let transaction = db.transaction().await?;
    let table = transaction.table::<MySwapsFiltersTable>().await?;
    let mut item = match table.get_item_by_unique_index("uuid", id).await? {
        Some((_item_id, item)) => item,
        None => return MmError::err(SwapStateMachineError::NoSwapWithUuid(id)),
    };
    item.is_finished = true.into();
    table.replace_item_by_unique_index("uuid", id, &item).await?;
    Ok(())
}

pub(super) fn init_additional_swaps_context(ctx: &MmArc, p2p_topic: String, uuid: Uuid) {
    subscribe_to_topic(ctx, p2p_topic);
    let swap_ctx = SwapsContext::from_ctx(ctx).expect("SwapsContext::from_ctx should not fail");
    swap_ctx.init_msg_v2_store(uuid, bits256::default());
}
