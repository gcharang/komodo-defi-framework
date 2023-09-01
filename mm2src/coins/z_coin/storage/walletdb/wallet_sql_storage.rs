use crate::z_coin::storage::WalletDbShared;
use crate::z_coin::{CheckPointBlockInfo, ZCoinBuilder, ZcoinClientInitError, ZcoinConsensusParams, ZcoinStorageError};
use common::async_blocking;
use db_common::sqlite::{query_single_row, run_optimization_pragmas};
use mm2_err_handle::prelude::*;
use std::path::PathBuf;
use zcash_client_sqlite::wallet::init::{init_accounts_table, init_blocks_table, init_wallet_db};
use zcash_client_sqlite::with_async::WalletDbAsync;
use zcash_extras::WalletRead;
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::BlockHeight;
use zcash_primitives::transaction::TxId;
use zcash_primitives::zip32::{ExtendedFullViewingKey, ExtendedSpendingKey};

pub async fn create_wallet_db(
    wallet_db_path: PathBuf,
    consensus_params: ZcoinConsensusParams,
    check_point_block: Option<CheckPointBlockInfo>,
    evk: ExtendedFullViewingKey,
) -> Result<WalletDbAsync<ZcoinConsensusParams>, MmError<ZcoinClientInitError>> {
    let db = WalletDbAsync::for_path(wallet_db_path, consensus_params)
        .map_to_mm(|err| ZcoinClientInitError::ZcashDBError(err.to_string()))?;
    let get_evk = db.get_extended_full_viewing_keys().await?;

    async_blocking(
        move || -> Result<WalletDbAsync<ZcoinConsensusParams>, MmError<ZcoinClientInitError>> {
            let conn = db.inner();
            let conn = conn.lock().unwrap();
            run_optimization_pragmas(conn.sql_conn())
                .map_to_mm(|err| ZcoinClientInitError::ZcashDBError(err.to_string()))?;
            init_wallet_db(&conn).map_to_mm(|err| ZcoinClientInitError::ZcashDBError(err.to_string()))?;

            if get_evk.is_empty() {
                init_accounts_table(&conn, &[evk])?;
                if let Some(check_point) = check_point_block {
                    init_blocks_table(
                        &conn,
                        BlockHeight::from_u32(check_point.height),
                        BlockHash(check_point.hash.0),
                        check_point.time,
                        &check_point.sapling_tree.0,
                    )?;
                }
            }

            Ok(db)
        },
    )
    .await
}

impl<'a> WalletDbShared {
    pub async fn new(
        zcoin_builder: &ZCoinBuilder<'a>,
        z_spending_key: &ExtendedSpendingKey,
    ) -> MmResult<Self, ZcoinStorageError> {
        let ticker = zcoin_builder.ticker;
        let wallet_db = create_wallet_db(
            zcoin_builder.db_dir_path.join(format!("{ticker}_wallet.db")),
            zcoin_builder.protocol_info.consensus_params.clone(),
            zcoin_builder.protocol_info.check_point_block.clone(),
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
            const QUERY: &str = "SELECT id_tx FROM transactions WHERE txid = ?1;";
            match query_single_row(conn.sql_conn(), QUERY, [tx_id.0.to_vec()], |row| row.get::<_, i64>(0)) {
                Ok(Some(_)) => true,
                Ok(None) | Err(_) => false,
            }
        })
        .await
    }
}
