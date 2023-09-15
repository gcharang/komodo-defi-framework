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
    use crate::z_coin::storage::{BlockDbImpl, BlockProcessingMode, DataConnStmtCacheWasm, DataConnStmtCacheWrapper};
    use crate::z_coin::{ValidateBlocksError, ZcoinConsensusParams, ZcoinStorageError};
    use crate::ZcoinProtocolInfo;
    use common::log::info;
    use common::log::wasm_log::register_wasm_log;
    use mm2_test_helpers::for_tests::mm_ctx_with_custom_db;
    use protobuf::Message;
    use wasm_bindgen_test::*;
    use zcash_client_backend::wallet::AccountId;
    use zcash_extras::fake_compact_block;
    use zcash_extras::fake_compact_block_spending;
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
              "sapling_activation_height": u32::from(sapling_activation_height()),
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

    async fn wallet_db_from_zcoin_builder_for_test(ticker: &str) -> WalletIndexedDb {
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
        blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                walletdb.get_max_height_hash().await.unwrap(),
                None,
            )
            .await
            .unwrap();

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
        let max_height_hash = walletdb.get_max_height_hash().await.unwrap();
        info!("HASH HEIGHT : {max_height_hash:?}");
        blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                max_height_hash,
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
        blockdb.insert_block(cb2.height as u32, cb_bytes).await.unwrap();

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

    #[wasm_bindgen_test]
    async fn invalid_chain_cache_disconnected() {
        // init blocks_db
        let ctx = mm_ctx_with_custom_db();
        let blockdb = BlockDbImpl::new(ctx, TICKER.to_string(), Some("")).await.unwrap();

        // init walletdb.
        let walletdb = wallet_db_from_zcoin_builder_for_test(TICKER).await;
        let consensus_params = consensus_params();

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        assert!(walletdb.init_accounts_table(&[extfvk.clone()]).await.is_ok());

        // Create some fake compactBlocks
        let (cb, _) = fake_compact_block(
            sapling_activation_height(),
            BlockHash([0; 32]),
            extfvk.clone(),
            Amount::from_u64(5).unwrap(),
        );
        let (cb2, _) = fake_compact_block(
            sapling_activation_height() + 1,
            cb.hash(),
            extfvk.clone(),
            Amount::from_u64(7).unwrap(),
        );
        let cb_bytes = cb.write_to_bytes().unwrap();
        blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();
        let cb2_bytes = cb2.write_to_bytes().unwrap();
        blockdb.insert_block(cb2.height as u32, cb2_bytes).await.unwrap();

        // Scan the cache again
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

        // Create more fake CompactBlocks that don't connect to the scanned ones
        let (cb3, _) = fake_compact_block(
            sapling_activation_height() + 2,
            BlockHash([1; 32]),
            extfvk.clone(),
            Amount::from_u64(8).unwrap(),
        );
        let (cb4, _) = fake_compact_block(
            sapling_activation_height() + 3,
            cb3.hash(),
            extfvk,
            Amount::from_u64(3).unwrap(),
        );
        let cb3_bytes = cb3.write_to_bytes().unwrap();
        blockdb.insert_block(cb3.height as u32, cb3_bytes).await.unwrap();
        let cb4_bytes = cb4.write_to_bytes().unwrap();
        blockdb.insert_block(cb4.height as u32, cb4_bytes).await.unwrap();

        // Data+cache chain should be invalid at the data/cache boundary
        let validate_chain = blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                walletdb.get_max_height_hash().await.unwrap(),
                None,
            )
            .await
            .unwrap_err();
        match validate_chain.get_inner() {
            ZcoinStorageError::ValidateBlocksError(ValidateBlocksError::ChainInvalid { height, .. }) => {
                assert_eq!(*height, sapling_activation_height() + 2)
            },
            _ => panic!(),
        }
    }

    #[wasm_bindgen_test]
    async fn test_invalid_chain_reorg() {
        // init blocks_db
        let ctx = mm_ctx_with_custom_db();
        let blockdb = BlockDbImpl::new(ctx, TICKER.to_string(), Some("")).await.unwrap();

        // init walletdb.
        let walletdb = wallet_db_from_zcoin_builder_for_test(TICKER).await;
        let consensus_params = consensus_params();

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        assert!(walletdb.init_accounts_table(&[extfvk.clone()]).await.is_ok());

        // Create some fake compactBlocks
        let (cb, _) = fake_compact_block(
            sapling_activation_height(),
            BlockHash([0; 32]),
            extfvk.clone(),
            Amount::from_u64(5).unwrap(),
        );
        let (cb2, _) = fake_compact_block(
            sapling_activation_height() + 1,
            cb.hash(),
            extfvk.clone(),
            Amount::from_u64(7).unwrap(),
        );
        let cb_bytes = cb.write_to_bytes().unwrap();
        blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();
        let cb2_bytes = cb2.write_to_bytes().unwrap();
        blockdb.insert_block(cb2.height as u32, cb2_bytes).await.unwrap();

        // Scan the cache again
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

        // Create more fake CompactBlocks that that contains a reorg
        let (cb3, _) = fake_compact_block(
            sapling_activation_height() + 2,
            cb2.hash(),
            extfvk.clone(),
            Amount::from_u64(8).unwrap(),
        );
        let (cb4, _) = fake_compact_block(
            sapling_activation_height() + 3,
            BlockHash([1; 32]),
            extfvk,
            Amount::from_u64(3).unwrap(),
        );
        let cb3_bytes = cb3.write_to_bytes().unwrap();
        blockdb.insert_block(cb3.height as u32, cb3_bytes).await.unwrap();
        let cb4_bytes = cb4.write_to_bytes().unwrap();
        blockdb.insert_block(cb4.height as u32, cb4_bytes).await.unwrap();

        // Data+cache chain should be invalid at the data/cache boundary
        let validate_chain = blockdb
            .process_blocks_with_mode(
                consensus_params.clone(),
                BlockProcessingMode::Validate,
                walletdb.get_max_height_hash().await.unwrap(),
                None,
            )
            .await
            .unwrap_err();
        match validate_chain.get_inner() {
            ZcoinStorageError::ValidateBlocksError(ValidateBlocksError::ChainInvalid { height, .. }) => {
                assert_eq!(*height, sapling_activation_height() + 3)
            },
            _ => panic!(),
        }
    }

    #[wasm_bindgen_test]
    async fn test_data_db_rewinding() {
        // init blocks_db
        let ctx = mm_ctx_with_custom_db();
        let blockdb = BlockDbImpl::new(ctx, TICKER.to_string(), Some("")).await.unwrap();

        // init walletdb.
        let walletdb = wallet_db_from_zcoin_builder_for_test(TICKER).await;
        let consensus_params = consensus_params();

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        assert!(walletdb.init_accounts_table(&[extfvk.clone()]).await.is_ok());

        // Account balance should be zero
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), Amount::zero());

        // Create some fake compactBlocks sending value to the address
        let value = Amount::from_u64(5).unwrap();
        let value2 = Amount::from_u64(7).unwrap();
        let (cb, _) = fake_compact_block(sapling_activation_height(), BlockHash([0; 32]), extfvk.clone(), value);
        let (cb2, _) = fake_compact_block(sapling_activation_height() + 1, cb.hash(), extfvk, value2);
        let cb_bytes = cb.write_to_bytes().unwrap();
        blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();
        let cb2_bytes = cb2.write_to_bytes().unwrap();
        blockdb.insert_block(cb2.height as u32, cb2_bytes).await.unwrap();

        // Scan the cache
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .unwrap();

        // Account balance should reflect both received notes
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value + value2);

        // Rewind to height of last scanned block
        walletdb
            .rewind_to_height(sapling_activation_height() + 1)
            .await
            .unwrap();

        // Account balance should should be unaltered
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value + value2);

        // Rewind so one block is dropped.
        walletdb.rewind_to_height(sapling_activation_height()).await.unwrap();

        // Account balance should only contain the first received note
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value);

        // Scan the cache again
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .unwrap();

        // Account balance should again reflect both received notes
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value + value2);
    }

    #[wasm_bindgen_test]
    async fn test_scan_cached_blocks_requires_sequential_blocks() {
        // init blocks_db
        let ctx = mm_ctx_with_custom_db();
        let blockdb = BlockDbImpl::new(ctx, TICKER.to_string(), Some("")).await.unwrap();

        // init walletdb.
        let walletdb = wallet_db_from_zcoin_builder_for_test(TICKER).await;
        let consensus_params = consensus_params();

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        assert!(walletdb.init_accounts_table(&[extfvk.clone()]).await.is_ok());

        // Create a block with height SAPLING_ACTIVATION_HEIGHT
        let value = Amount::from_u64(50000).unwrap();
        let (cb1, _) = fake_compact_block(sapling_activation_height(), BlockHash([0; 32]), extfvk.clone(), value);
        let cb1_bytes = cb1.write_to_bytes().unwrap();
        blockdb.insert_block(cb1.height as u32, cb1_bytes).await.unwrap();

        // Scan cache
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .unwrap();

        // We cannot scan a block of height SAPLING_ACTIVATION_HEIGHT + 2 next
        let (cb2, _) = fake_compact_block(sapling_activation_height() + 1, cb1.hash(), extfvk.clone(), value);
        let cb2_bytes = cb2.write_to_bytes().unwrap();
        let (cb3, _) = fake_compact_block(sapling_activation_height() + 2, cb2.hash(), extfvk.clone(), value);
        let cb3_bytes = cb3.write_to_bytes().unwrap();
        blockdb.insert_block(cb3.height as u32, cb3_bytes).await.unwrap();
        // Scan the cache again
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        let scan = blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .unwrap_err();
        match scan.get_inner() {
            ZcoinStorageError::ValidateBlocksError(err) => {
                let actual = err.to_string();
                let expected = ValidateBlocksError::block_height_discontinuity(
                    sapling_activation_height() + 1,
                    sapling_activation_height() + 2,
                );
                assert_eq!(expected.to_string(), actual)
            },
            _ => panic!("Should have failed"),
        }

        // if we add a block of height SPALING_ACTIVATION_HEIGHT +!, we can now scan both;
        blockdb.insert_block(cb2.height as u32, cb2_bytes).await.unwrap();
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        assert!(blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .is_ok());

        assert_eq!(
            walletdb.get_balance(AccountId(0)).await.unwrap(),
            Amount::from_u64(150_000).unwrap()
        );
    }

    #[wasm_bindgen_test]
    async fn test_scan_cached_blokcs_finds_received_notes() {
        // init blocks_db
        let ctx = mm_ctx_with_custom_db();
        let blockdb = BlockDbImpl::new(ctx, TICKER.to_string(), Some("")).await.unwrap();

        // init walletdb.
        let walletdb = wallet_db_from_zcoin_builder_for_test(TICKER).await;
        let consensus_params = consensus_params();

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        assert!(walletdb.init_accounts_table(&[extfvk.clone()]).await.is_ok());

        // Account balance should be zero
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), Amount::zero());

        // Create a fake compactblock sending value to the address
        let value = Amount::from_u64(5).unwrap();
        let (cb1, _) = fake_compact_block(sapling_activation_height(), BlockHash([0; 32]), extfvk.clone(), value);
        let cb1_bytes = cb1.write_to_bytes().unwrap();
        blockdb.insert_block(cb1.height as u32, cb1_bytes).await.unwrap();

        // Scan the cache
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        assert!(blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .is_ok());

        // Account balance should reflect the received note
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value);

        // Create a second fake Compactblock sending more value to the address
        let value2 = Amount::from_u64(7).unwrap();
        let (cb2, _) = fake_compact_block(sapling_activation_height() + 1, cb1.hash(), extfvk.clone(), value2);
        let cb2_bytes = cb2.write_to_bytes().unwrap();
        blockdb.insert_block(cb2.height as u32, cb2_bytes).await.unwrap();

        // Scan the cache again
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        assert!(blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .is_ok());

        // Account balance should reflect the received note
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value + value2);
    }

    #[wasm_bindgen_test]
    async fn test_scan_cached_blocks_finds_change_notes() {
        // init blocks_db
        let ctx = mm_ctx_with_custom_db();
        let blockdb = BlockDbImpl::new(ctx, TICKER.to_string(), Some("")).await.unwrap();

        // init walletdb.
        let walletdb = wallet_db_from_zcoin_builder_for_test(TICKER).await;
        let consensus_params = consensus_params();

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        assert!(walletdb.init_accounts_table(&[extfvk.clone()]).await.is_ok());

        // Account balance should be zero
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), Amount::zero());

        // Create a fake compactblock sending value to the address
        let value = Amount::from_u64(5).unwrap();
        let (cb1, nf) = fake_compact_block(sapling_activation_height(), BlockHash([0; 32]), extfvk.clone(), value);
        let cb1_bytes = cb1.write_to_bytes().unwrap();
        blockdb.insert_block(cb1.height as u32, cb1_bytes).await.unwrap();

        // Scan the cache
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        assert!(blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .is_ok());

        // Account balance should reflect the received note
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value);

        // Create a second fake Compactblock spending value from the address
        let extsk2 = ExtendedSpendingKey::master(&[0]);
        let to2 = extsk2.default_address().unwrap().1;
        let value2 = Amount::from_u64(2).unwrap();
        let cb2 = fake_compact_block_spending(
            sapling_activation_height() + 1,
            cb1.hash(),
            (nf, value),
            extfvk,
            to2,
            value2,
        );
        let cb2_bytes = cb2.write_to_bytes().unwrap();
        blockdb.insert_block(cb2.height as u32, cb2_bytes).await.unwrap();

        // Scan the cache again
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        let scan = blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await;
        info!("SCAN: {scan:?}");
        assert!(scan.is_ok());

        info!("extrema {:?}", walletdb.block_height_extrema().await.unwrap());
        // Account balance should equal the change
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value - value2);
    }

    fn network() -> Network { Network::TestNetwork }

    fn test_prover() -> impl TxProver {
        match LocalTxProver::with_default_location() {
            Some(tx_prover) => tx_prover,
            None => {
                panic!("Cannot locate the Zcash parameters. Please run zcash-fetch-params or fetch-params.sh to download the parameters, and then re-run the tests.");
            },
        }
    }

    #[wasm_bindgen_test]
    async fn create_to_address_fails_on_unverified_notes() {
        register_wasm_log();

        // init blocks_db
        let ctx = mm_ctx_with_custom_db();
        let blockdb = BlockDbImpl::new(ctx, TICKER.to_string(), Some("")).await.unwrap();

        // init walletdb.
        let walletdb = wallet_db_from_zcoin_builder_for_test(TICKER).await;
        let consensus_params = consensus_params();

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvk = ExtendedFullViewingKey::from(&extsk);
        assert!(walletdb.init_accounts_table(&[extfvk.clone()]).await.is_ok());

        // Account balance should be zero
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), Amount::zero());

        // Add funds to the wallet in a single note
        let value = Amount::from_u64(50000).unwrap();
        let (cb, _) = fake_compact_block(sapling_activation_height(), BlockHash([0; 32]), extfvk.clone(), value);
        let cb_bytes = cb.write_to_bytes().unwrap();
        blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();

        // Scan the cache
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        assert!(blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .is_ok());

        // Verified balance matches total balance
        let (_, anchor_height) = walletdb.get_target_and_anchor_heights().await.unwrap().unwrap();
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value);
        assert_eq!(
            walletdb.get_balance_at(AccountId(0), anchor_height).await.unwrap(),
            value
        );

        // Add more funds to the wallet in a second note
        let (cb, _) = fake_compact_block(sapling_activation_height() + 1, cb.hash(), extfvk.clone(), value);
        let cb_bytes = cb.write_to_bytes().unwrap();
        blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();

        // Verified balance does not include the second note
        let (_, anchor_height2) = (&db_data).get_target_and_anchor_heights().unwrap().unwrap();
        assert_eq!(walletdb.get_balance(AccountId(0)).await.unwrap(), value + value);
        assert_eq!(
            walletdb.get_balance_at(AccountId(0), anchor_height).await.unwrap(),
            value
        );

        // Spend fails because there are insufficient verified notes
        let extsk2 = ExtendedSpendingKey::master(&[]);
        let to = extsk2.default_address().unwrap().1.into();
        match create_spend_to_address(
            &mut walletdb,
            network(),
            test_prover(),
            AccountId(0),
            &extsk,
            &to,
            Amount::from_u64(70000).unwrap(),
            None,
            OvkPolicy::Sender,
        )
        .await
        {
            Ok(_) => panic!("Should have failed"),
            Err(e) => assert_eq!(
                e.to_string(),
                "Insufficient balance (have 50000, need 71000 including fee)"
            ),
        }

        // Mine blocks SAPLING_ACTIVATION_HEIGHT + 2 to 9 until just before the second
        // note is verified
        for i in 2..10 {
            let (cb, _) = fake_compact_block(sapling_activation_height() + i, cb.hash(), extfvk.clone(), value);
            let cb_bytes = cb.write_to_bytes().unwrap();
            blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();
        }

        // Scan the cache
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        assert!(blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .is_ok());

        // Second spend still fails
        match create_spend_to_address(
            &mut walletdb,
            network(),
            test_prover(),
            AccountId(0),
            &extsk,
            &to,
            Amount::from_u64(70000).unwrap(),
            None,
            OvkPolicy::Sender,
        ) {
            Ok(_) => panic!("Should have failed"),
            Err(e) => assert_eq!(
                e.to_string(),
                "Insufficient balance (have 50000, need 71000 including fee)"
            ),
        }

        // Mine block 11 so that the second note becomes verified
        let (cb, _) = fake_compact_block(sapling_activation_height() + 10, cb.hash(), extfvk, value);
        let cb_bytes = cb.write_to_bytes().unwrap();
        blockdb.insert_block(cb.height as u32, cb_bytes).await.unwrap();
        // Scan the cache
        let scan = DataConnStmtCacheWrapper::new(DataConnStmtCacheWasm(walletdb.clone()));
        assert!(blockdb
            .process_blocks_with_mode(consensus_params.clone(), BlockProcessingMode::Scan(scan), None, None)
            .await
            .is_ok());

        // Second spend should now succeed
        create_spend_to_address(
            &mut walletdb,
            network(),
            test_prover(),
            AccountId(0),
            &extsk,
            &to,
            Amount::from_u64(70000).unwrap(),
            None,
            OvkPolicy::Sender,
        )
        .unwrap();
    }
}
