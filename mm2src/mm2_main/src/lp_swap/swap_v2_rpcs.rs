use super::maker_swap::MakerSavedSwap;
use super::taker_swap::TakerSavedSwap;
use crate::mm2::lp_swap::maker_swap_v2::MakerSwapEvent;
use crate::mm2::lp_swap::taker_swap_v2::TakerSwapEvent;
use common::HttpStatusCode;
use derive_more::Display;
use http::StatusCode;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use mm2_number::{MmNumber, MmNumberMultiRepr};
use serde::de::DeserializeOwned;
use uuid::Uuid;

cfg_native!(
    use crate::mm2::database::my_swaps::SELECT_MY_SWAP_V2_FOR_RPC_BY_UUID;
    use common::async_blocking;
    use db_common::sqlite::rusqlite::{Result as SqlResult, Row, Error as SqlError};
    use db_common::sqlite::rusqlite::types::Type as SqlType;
);

cfg_wasm32!(
    use super::SwapsContext;
    use super::maker_swap_v2::MakerSwapDbRepr;
    use super::taker_swap_v2::TakerSwapDbRepr;
    use crate::mm2::lp_swap::swap_wasm_db::{MySwapsFiltersTable, SavedSwapTable};
    use mm2_db::indexed_db::{DbTransactionError, DbTransactionResult, InitDbError};
);

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn get_swap_type(ctx: &MmArc, uuid: &Uuid) -> MmResult<u8, SqlError> {
    let ctx = ctx.clone();
    let uuid = uuid.to_string();

    async_blocking(move || {
        let conn = ctx.sqlite_connection();
        const SELECT_SWAP_TYPE_BY_UUID: &str = "SELECT swap_type FROM my_swaps WHERE uuid = :uuid;";
        let mut stmt = conn.prepare(SELECT_SWAP_TYPE_BY_UUID)?;
        let swap_type = stmt.query_row(&[(":uuid", uuid.as_str())], |row| row.get(0))?;
        Ok(swap_type)
    })
    .await
}

#[cfg(target_arch = "wasm32")]
#[derive(Display)]
pub enum SwapV2DbError {
    DbTransaction(DbTransactionError),
    InitDb(InitDbError),
    Serde(serde_json::Error),
    NoSwapWithUuid(Uuid),
    UnsupportedSwapType(u8),
}

#[cfg(target_arch = "wasm32")]
impl From<DbTransactionError> for SwapV2DbError {
    fn from(e: DbTransactionError) -> Self { SwapV2DbError::DbTransaction(e) }
}

#[cfg(target_arch = "wasm32")]
impl From<InitDbError> for SwapV2DbError {
    fn from(e: InitDbError) -> Self { SwapV2DbError::InitDb(e) }
}

#[cfg(target_arch = "wasm32")]
impl From<serde_json::Error> for SwapV2DbError {
    fn from(e: serde_json::Error) -> Self { SwapV2DbError::Serde(e) }
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn get_swap_type(ctx: &MmArc, uuid: &Uuid) -> MmResult<u8, SwapV2DbError> {
    use crate::mm2::lp_swap::swap_wasm_db::MySwapsFiltersTable;

    let swaps_ctx = SwapsContext::from_ctx(ctx).unwrap();
    let db = swaps_ctx.swap_db().await?;
    let transaction = db.transaction().await?;
    let table = transaction.table::<MySwapsFiltersTable>().await?;
    let item = match table.get_item_by_unique_index("uuid", uuid).await? {
        Some((_item_id, item)) => item,
        None => return MmError::err(SwapV2DbError::NoSwapWithUuid(*uuid)),
    };
    Ok(item.swap_type)
}

/// Represents data of the swap used for RPC, omits fields that should be kept in secret
#[derive(Debug, Serialize)]
pub(crate) struct MySwapForRpc<T> {
    my_coin: String,
    other_coin: String,
    uuid: Uuid,
    started_at: i64,
    is_finished: bool,
    events: Vec<T>,
    maker_volume: MmNumberMultiRepr,
    taker_volume: MmNumberMultiRepr,
    premium: MmNumberMultiRepr,
    dex_fee: MmNumberMultiRepr,
    lock_duration: i64,
    maker_coin_confs: i64,
    maker_coin_nota: bool,
    taker_coin_confs: i64,
    taker_coin_nota: bool,
}

impl<T: DeserializeOwned> MySwapForRpc<T> {
    #[cfg(not(target_arch = "wasm32"))]
    fn from_row(row: &Row) -> SqlResult<Self> {
        Ok(Self {
            my_coin: row.get(0)?,
            other_coin: row.get(1)?,
            uuid: row
                .get::<_, String>(2)?
                .parse()
                .map_err(|e| SqlError::FromSqlConversionFailure(2, SqlType::Text, Box::new(e)))?,
            started_at: row.get(3)?,
            is_finished: row.get(4)?,
            events: serde_json::from_str(&row.get::<_, String>(5)?)
                .map_err(|e| SqlError::FromSqlConversionFailure(5, SqlType::Text, Box::new(e)))?,
            maker_volume: MmNumber::from_fraction_string(&row.get::<_, String>(6)?)
                .map_err(|e| SqlError::FromSqlConversionFailure(6, SqlType::Text, Box::new(e)))?
                .into(),
            taker_volume: MmNumber::from_fraction_string(&row.get::<_, String>(7)?)
                .map_err(|e| SqlError::FromSqlConversionFailure(7, SqlType::Text, Box::new(e)))?
                .into(),
            premium: MmNumber::from_fraction_string(&row.get::<_, String>(8)?)
                .map_err(|e| SqlError::FromSqlConversionFailure(8, SqlType::Text, Box::new(e)))?
                .into(),
            dex_fee: MmNumber::from_fraction_string(&row.get::<_, String>(9)?)
                .map_err(|e| SqlError::FromSqlConversionFailure(9, SqlType::Text, Box::new(e)))?
                .into(),
            lock_duration: row.get(10)?,
            maker_coin_confs: row.get(11)?,
            maker_coin_nota: row.get(12)?,
            taker_coin_confs: row.get(13)?,
            taker_coin_nota: row.get(14)?,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn get_maker_swap_data_for_rpc(
    ctx: &MmArc,
    uuid: &Uuid,
) -> MmResult<MySwapForRpc<MakerSwapEvent>, SqlError> {
    get_swap_data_for_rpc_impl(ctx, uuid).await
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn get_taker_swap_data_for_rpc(
    ctx: &MmArc,
    uuid: &Uuid,
) -> MmResult<MySwapForRpc<TakerSwapEvent>, SqlError> {
    get_swap_data_for_rpc_impl(ctx, uuid).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn get_swap_data_for_rpc_impl<T: DeserializeOwned + Send + 'static>(
    ctx: &MmArc,
    uuid: &Uuid,
) -> MmResult<MySwapForRpc<T>, SqlError> {
    let ctx = ctx.clone();
    let uuid = uuid.to_string();

    async_blocking(move || {
        let conn = ctx.sqlite_connection();
        let mut stmt = conn.prepare(SELECT_MY_SWAP_V2_FOR_RPC_BY_UUID)?;
        let swap_data = stmt.query_row(&[(":uuid", uuid.as_str())], MySwapForRpc::from_row)?;
        Ok(swap_data)
    })
    .await
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn get_maker_swap_data_for_rpc(
    ctx: &MmArc,
    uuid: &Uuid,
) -> MmResult<MySwapForRpc<MakerSwapEvent>, SwapV2DbError> {
    let swaps_ctx = SwapsContext::from_ctx(ctx).unwrap();
    let db = swaps_ctx.swap_db().await?;
    let transaction = db.transaction().await?;
    let table = transaction.table::<SavedSwapTable>().await?;
    let item = match table.get_item_by_unique_index("uuid", uuid).await? {
        Some((_item_id, item)) => item,
        None => return MmError::err(SwapV2DbError::NoSwapWithUuid(*uuid)),
    };

    let filters_table = transaction.table::<MySwapsFiltersTable>().await?;
    let filter_item = match filters_table.get_item_by_unique_index("uuid", uuid).await? {
        Some((_item_id, item)) => item,
        None => return MmError::err(SwapV2DbError::NoSwapWithUuid(*uuid)),
    };

    let json_repr: MakerSwapDbRepr = serde_json::from_value(item.saved_swap)?;
    Ok(MySwapForRpc {
        my_coin: json_repr.maker_coin,
        other_coin: json_repr.taker_coin,
        uuid: json_repr.uuid,
        started_at: json_repr.started_at as i64,
        is_finished: filter_item.is_finished.as_bool(),
        events: json_repr.events,
        maker_volume: json_repr.maker_volume.into(),
        taker_volume: json_repr.taker_volume.into(),
        premium: json_repr.taker_premium.into(),
        dex_fee: json_repr.dex_fee_amount.into(),
        lock_duration: json_repr.lock_duration as i64,
        maker_coin_confs: json_repr.conf_settings.maker_coin_confs as i64,
        maker_coin_nota: json_repr.conf_settings.maker_coin_nota,
        taker_coin_confs: json_repr.conf_settings.taker_coin_confs as i64,
        taker_coin_nota: json_repr.conf_settings.taker_coin_nota,
    })
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn get_taker_swap_data_for_rpc(
    ctx: &MmArc,
    uuid: &Uuid,
) -> MmResult<MySwapForRpc<TakerSwapEvent>, SwapV2DbError> {
    let swaps_ctx = SwapsContext::from_ctx(ctx).unwrap();
    let db = swaps_ctx.swap_db().await?;
    let transaction = db.transaction().await?;
    let table = transaction.table::<SavedSwapTable>().await?;
    let item = match table.get_item_by_unique_index("uuid", uuid).await? {
        Some((_item_id, item)) => item,
        None => return MmError::err(SwapV2DbError::NoSwapWithUuid(*uuid)),
    };

    let filters_table = transaction.table::<MySwapsFiltersTable>().await?;
    let filter_item = match filters_table.get_item_by_unique_index("uuid", uuid).await? {
        Some((_item_id, item)) => item,
        None => return MmError::err(SwapV2DbError::NoSwapWithUuid(*uuid)),
    };

    let json_repr: TakerSwapDbRepr = serde_json::from_value(item.saved_swap)?;
    Ok(MySwapForRpc {
        my_coin: json_repr.taker_coin,
        other_coin: json_repr.maker_coin,
        uuid: json_repr.uuid,
        started_at: json_repr.started_at as i64,
        is_finished: filter_item.is_finished.as_bool(),
        events: json_repr.events,
        maker_volume: json_repr.maker_volume.into(),
        taker_volume: json_repr.taker_volume.into(),
        premium: json_repr.taker_premium.into(),
        dex_fee: json_repr.dex_fee.into(),
        lock_duration: json_repr.lock_duration as i64,
        maker_coin_confs: json_repr.conf_settings.maker_coin_confs as i64,
        maker_coin_nota: json_repr.conf_settings.maker_coin_nota,
        taker_coin_confs: json_repr.conf_settings.taker_coin_confs as i64,
        taker_coin_nota: json_repr.conf_settings.taker_coin_nota,
    })
}

#[derive(Serialize)]
pub(crate) enum SwapRpcData {
    MakerV1(MakerSavedSwap),
    TakerV1(TakerSavedSwap),
    MakerV2(MySwapForRpc<MakerSwapEvent>),
    TakerV2(MySwapForRpc<TakerSwapEvent>),
}

#[derive(Deserialize)]
pub(crate) struct MySwapStatusRequest {
    uuid: Uuid,
}

#[derive(Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub(crate) enum MySwapStatusError {}

impl HttpStatusCode for MySwapStatusError {
    fn status_code(&self) -> StatusCode { todo!() }
}

pub(crate) async fn my_swap_status_rpc(
    ctx: MmArc,
    req: MySwapStatusRequest,
) -> MmResult<SwapRpcData, MySwapStatusError> {
    unimplemented!()
}
