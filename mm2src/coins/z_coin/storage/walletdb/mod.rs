cfg_native!(
    use crate::z_coin::ZcoinConsensusParams;

    pub mod wallet_sql_storage;
    use zcash_client_sqlite::with_async::WalletDbAsync;
);

#[cfg(target_arch = "wasm32")] pub mod wallet_idb_storage;

use crate::z_coin::{CheckPointBlockInfo, ZcoinClientInitError};
use mm2_err_handle::prelude::MmError;
#[cfg(target_arch = "wasm32")]
use wallet_idb_storage::WalletIndexedDb;
use zcash_primitives::consensus::BlockHeight;

#[derive(Clone)]
pub struct WalletDbShared {
    #[cfg(not(target_arch = "wasm32"))]
    pub db: WalletDbAsync<ZcoinConsensusParams>,
    #[cfg(target_arch = "wasm32")]
    pub db: WalletIndexedDb,
    #[allow(unused)]
    ticker: String,
}

async fn is_init_height_modified(
    extrema: Option<(BlockHeight, BlockHeight)>,
    checkpoint_block: &Option<CheckPointBlockInfo>,
) -> Result<(bool, Option<u32>), MmError<ZcoinClientInitError>> {
    let min_sync_height = extrema.map(|(min, _)| u32::from(min));
    let init_block_height = checkpoint_block.as_ref().map(|block| block.height);

    Ok((init_block_height != min_sync_height, init_block_height))
}

#[cfg(target_arch = "wasm32")]
mod wallet_db_storage_tests {
    use super::*;
    use crate::z_coin::storage::DataConnStmtCacheWasm;
    use crate::z_coin::storage::DataConnStmtCacheWrapper;
    use crate::z_coin::storage::{BlockDbImpl, BlockProcessingMode};
    use crate::z_coin::ZcoinConsensusParams;
    use crate::ZcoinProtocolInfo;
    use common::log::info;
    use common::log::wasm_log::register_wasm_log;
    use mm2_test_helpers::for_tests::mm_ctx_with_custom_db;
    use protobuf::Message;
    use wasm_bindgen_test::*;
    use zcash_client_backend::wallet::AccountId;
    use zcash_extras::fake_compact_block;
    use zcash_extras::WalletRead;
    use zcash_primitives::block::BlockHash;
    use zcash_primitives::consensus::{Network, NetworkUpgrade, Parameters};
    use zcash_primitives::transaction::components::Amount;
    use zcash_primitives::zip32::{ExtendedFullViewingKey, ExtendedSpendingKey};

    wasm_bindgen_test_configure!(run_in_browser);

    const TICKER: &str = "ARRR";

    fn consensus_params() -> ZcoinConsensusParams {
        let protocol_info = serde_json::from_value::<ZcoinProtocolInfo>(json!({
            "consensus_params": {
              "overwinter_activation_height": 152855,
              "sapling_activation_height": 152855,
              "blossom_activation_height": null,
              "heartwood_activation_height": null,
              "canopy_activation_height": null,
              "coin_type": 133,
              "hrp_sapling_extended_spending_key": "secret-extended-key-main",
              "hrp_sapling_extended_full_viewing_key": "zxviews",
              "hrp_sapling_payment_address": "zs",
              "b58_pubkey_address_prefix": [
                28,
                184
              ],
              "b58_script_address_prefix": [
                28,
                189
              ]
            }
        }))
        .unwrap();

        protocol_info.consensus_params
    }

    pub fn sapling_activation_height() -> BlockHeight {
        Network::TestNetwork.activation_height(NetworkUpgrade::Sapling).unwrap()
    }

    async fn wallet_db_from_zcoin_builder_for_test<'a>(ticker: &'a str) -> WalletIndexedDb {
        let ctx = mm_ctx_with_custom_db();
        let consensus_params = consensus_params();

        WalletIndexedDb::new(&ctx, ticker, consensus_params).await.unwrap()
    }

    #[wasm_bindgen_test]
    async fn test_empty_database_has_no_balance() {
        let db = wallet_db_from_zcoin_builder_for_test(TICKER).await;

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvks = [ExtendedFullViewingKey::from(&extsk)];
        assert!(db.init_accounts_table(&extfvks).await.is_ok());

        // The account should be empty
        assert_eq!(db.get_balance(AccountId(0)).await.unwrap(), Amount::zero());

        // We can't get an anchor height, as we have not scanned any blocks.
        assert_eq!(db.get_target_and_anchor_heights().await.unwrap(), None);

        // An invalid account has zero balance
        assert!(db.get_address(AccountId(1)).await.is_err());
        assert_eq!(db.get_balance(AccountId(0)).await.unwrap(), Amount::zero());
    }

    #[wasm_bindgen_test]
    async fn test_init_accounts_table_only_works_once() {
        let db = wallet_db_from_zcoin_builder_for_test(TICKER).await;

        // We can call the function as many times as we want with no data
        assert!(db.init_accounts_table(&[]).await.is_ok());
        assert!(db.init_accounts_table(&[]).await.is_ok());

        // First call with data should initialise the accounts table.
        let extfvks = [ExtendedFullViewingKey::from(&ExtendedSpendingKey::master(&[]))];
        assert!(db.init_accounts_table(&extfvks).await.is_ok());

        // Subsequent calls should return an error
        assert!(db.init_accounts_table(&extfvks).await.is_ok());
    }

    #[wasm_bindgen_test]
    async fn test_init_blocks_table_only_works_once() {
        let db = wallet_db_from_zcoin_builder_for_test(TICKER).await;

        // First call with data should initialise the blocks table
        assert!(db
            .init_blocks_table(BlockHeight::from(1), BlockHash([1; 32]), 1, &[])
            .await
            .is_ok());

        // Subsequent calls should return an error
        assert!(db
            .init_blocks_table(BlockHeight::from(2), BlockHash([2; 32]), 2, &[])
            .await
            .is_err());
    }

    #[wasm_bindgen_test]
    async fn init_accounts_table_stores_correct_address() {
        let db = wallet_db_from_zcoin_builder_for_test(TICKER).await;

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvks = [ExtendedFullViewingKey::from(&extsk)];
        assert!(db.init_accounts_table(&extfvks).await.is_ok());

        // The account's address should be in the data DB.
        let pa = db.get_address(AccountId(0)).await.unwrap();
        info!("address: {pa:?}");
        assert_eq!(pa.unwrap(), extsk.default_address().unwrap().1);
    }

    #[wasm_bindgen_test]
    async fn test_valid_chain_state() {
        register_wasm_log();

        // init blocks_db
        let ctx = mm_ctx_with_custom_db();
        let blockdb = BlockDbImpl::new(ctx, TICKER.to_string(), Some("")).await.unwrap();

        // init walletdb.
        let walletdb = wallet_db_from_zcoin_builder_for_test(TICKER).await;

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        assert!(walletdb.init_accounts_table(&[extfvk.clone()]).await.is_ok());

        // Empty chain should be valid
        let consensus_params = consensus_params();
        let process_validate = blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                walletdb.get_max_height_hash().await.unwrap(),
                None,
            )
            .await
            .unwrap();
        info!("{process_validate:?}");

        // create a fake compactBlock sending value to the address
        let (cb, _) = fake_compact_block(
            sapling_activation_height(),
            BlockHash([0; 32]),
            extfvk.clone(),
            Amount::from_u64(5).unwrap(),
        );
        let cb_bytes = cb.write_to_bytes().unwrap();
        blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();

        // Cache-only chain should be valid
        blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                walletdb.get_max_height_hash().await.unwrap(),
                None,
            )
            .await
            .unwrap();

        // scan the cache
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .unwrap();

        // Data-only chain should be valid
        blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                walletdb.get_max_height_hash().await.unwrap(),
                None,
            )
            .await
            .unwrap();

        // Create a second fake CompactBlock sending more value to the address
        let (cb2, _) = fake_compact_block(
            sapling_activation_height() + 1,
            cb.hash(),
            extfvk,
            Amount::from_u64(7).unwrap(),
        );
        let cb_bytes = cb2.write_to_bytes().unwrap();
        blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();

        // Data+cache chain should be valid
        blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                walletdb.get_max_height_hash().await.unwrap(),
                None,
            )
            .await
            .unwrap();

        // Scan the cache again
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .unwrap();

        // Data+cache chain should be valid
        blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                walletdb.get_max_height_hash().await.unwrap(),
                None,
            )
            .await
            .unwrap();
    }
}
