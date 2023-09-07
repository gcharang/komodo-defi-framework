use crate::z_coin::storage::WalletDbShared;
use crate::z_coin::{CheckPointBlockInfo, ZCoinBuilder, ZcoinClientInitError, ZcoinConsensusParams, ZcoinStorageError};
use common::async_blocking;
use common::log::info;
use db_common::sqlite::{query_single_row, run_optimization_pragmas};
use mm2_err_handle::prelude::*;
use std::path::PathBuf;
use zcash_client_sqlite::with_async::init::{init_accounts_table, init_blocks_table, init_wallet_db};
use zcash_client_sqlite::with_async::WalletDbAsync;
use zcash_extras::{WalletRead, WalletWrite};
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::BlockHeight;
use zcash_primitives::transaction::TxId;
use zcash_primitives::zip32::{ExtendedFullViewingKey, ExtendedSpendingKey};

fn run_optimization_pragmas_helper(w: &WalletDbAsync<ZcoinConsensusParams>) -> MmResult<(), ZcoinClientInitError> {
    let conn = w.inner();
    let conn = conn.lock().unwrap();
    run_optimization_pragmas(conn.sql_conn()).map_to_mm(|err| ZcoinClientInitError::ZcashDBError(err.to_string()))?;
    Ok(())
}

/// `create_wallet_db` is responsible for creating a new Zcoin wallet database, initializing it
/// with the provided parameters, and executing various initialization steps. These steps include checking and
/// potentially rewinding the database to a specified synchronization height, performing optimizations, and
/// setting up the initial state of the wallet database.
pub async fn create_wallet_db(
    wallet_db_path: PathBuf,
    consensus_params: ZcoinConsensusParams,
    checkpoint_block: Option<CheckPointBlockInfo>,
    evk: ExtendedFullViewingKey,
) -> Result<WalletDbAsync<ZcoinConsensusParams>, MmError<ZcoinClientInitError>> {
    let db = WalletDbAsync::for_path(wallet_db_path, consensus_params)
        .map_to_mm(|err| ZcoinClientInitError::ZcashDBError(err.to_string()))?;

    run_optimization_pragmas_helper(&db)?;
    init_wallet_db(&db).map_to_mm(|err| ZcoinClientInitError::ZcashDBError(err.to_string()))?;

    let extrema = db.block_height_extrema().await?;
    let min_sync_height = extrema.map(|(min, _)| u32::from(min));
    let init_block_height = checkpoint_block.clone().map(|block| block.height);

    // Check if the initial block height is less than the previous synchronization height and
    // Rewind walletdb to the minimum possible height.
    let get_evk = db.get_extended_full_viewing_keys().await?;
    if get_evk.is_empty() || init_block_height != min_sync_height {
        info!("Older/Newer sync height detected!, rewinding walletdb to new height: {init_block_height:?}");
        let mut wallet_ops = db.get_update_ops().expect("get_update_ops always returns Ok");
        wallet_ops
            .rewind_to_height(u32::MIN.into())
            .await
            .map_to_mm(|err| ZcoinClientInitError::ZcashDBError(err.to_string()))?;
        if let Some(block) = checkpoint_block.clone() {
            init_blocks_table(
                &db,
                BlockHeight::from_u32(block.height),
                BlockHash(block.hash.0),
                block.time,
                &block.sapling_tree.0,
            )?;
        }
    }

    if get_evk.is_empty() {
        init_accounts_table(&db, &[evk])?;
    }

    Ok(db)
}

impl<'a> WalletDbShared {
    pub async fn new(
        zcoin_builder: &ZCoinBuilder<'a>,
        checkpoint_block: Option<CheckPointBlockInfo>,
        z_spending_key: &ExtendedSpendingKey,
    ) -> MmResult<Self, ZcoinStorageError> {
        let ticker = zcoin_builder.ticker;
        let wallet_db = create_wallet_db(
            zcoin_builder.db_dir_path.join(format!("{ticker}_wallet.db")),
            zcoin_builder.protocol_info.consensus_params.clone(),
            checkpoint_block,
            ExtendedFullViewingKey::from(z_spending_key),
        )
        .await
        .map_err(|err| ZcoinStorageError::InitDbError {
            ticker: ticker.to_string(),
            err: err.to_string(),
        })?;

        Ok(Self {
            db: wallet_db,
            ticker: ticker.to_string(),
        })
    }

    pub async fn is_tx_imported(&self, tx_id: TxId) -> bool {
        let db = self.db.inner();
        async_blocking(move || {
            let conn = db.lock().unwrap();
            const QUERY: &str = "SELECT EXISTS (SELECT 1 FROM transactions WHERE txid = ?1);";
            match query_single_row(conn.sql_conn(), QUERY, [tx_id.0.to_vec()], |row| row.get::<_, i64>(0)) {
                Ok(Some(_)) => true,
                Ok(None) | Err(_) => false,
            }
        })
        .await
    }
}
