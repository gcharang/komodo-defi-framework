cfg_native!(
    use crate::z_coin::ZcoinConsensusParams;

    pub mod wallet_sql_storage;
    use zcash_client_sqlite::with_async::WalletDbAsync;
);

#[cfg(target_arch = "wasm32")] pub mod wallet_idb_storage;

use crate::z_coin::{extended_spending_key_from_protocol_info_and_policy, CheckPointBlockInfo, ZCoinBuilder,
                    ZcoinClientInitError};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::MmError;
use std::path::PathBuf;
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

use crate::z_coin::{ZcoinActivationParams, ZcoinProtocolInfo};
use crate::PrivKeyBuildPolicy;

#[cfg(target_arch = "wasm32")]
async fn wallet_db_from_zcoin_builder_for_test<'a>(ctx: &'a MmArc, ticker: &'a str) -> WalletDbShared {
    let activation_params = serde_json::from_value::<ZcoinActivationParams>(json!({"mode": {
        "rpc": "Light",
        "rpc_data": {
            "electrum_servers": [
                {
                    "url": "zombie.dragonhound.info:10033"
                }
            ],
            "light_wallet_d_servers": [
                "https://pirate.spyglass.quest:9447"
            ],
            "sync_params": {
                "height": 2563000
            }
        }
    }}))
    .unwrap();
    let conf = json!({
        "coin": "ARRR",
        "asset": "PIRATE",
        "fname": "Pirate",
        "txversion": 4,
        "overwintered": 1,
        "mm2": 1,
        "avg_blocktime": 60
    });
    let protocol_info = serde_json::from_value::<ZcoinProtocolInfo>(json!({"protocol": {
          "type": "ZHTLC",
          "protocol_data": {
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
            },
            "check_point_block": {
              "height": 1900000,
              "time": 1652512363,
              "hash": "44797f3bb78323a7717007f1e289a3689e0b5b3433385dbd8e6f6a1700000000",
              "sapling_tree": "01e40c26f4a28071535b95ae637d30a209531e92a33de0a649e51183771025fd0f016cdc51442fcb328d047a709dc0f41e0173953404711045b3ef3036d7fd4151271501d6c94c5ce6787826af809aaee83768c4b7d4f02c8dc2d24cf60ed5f127a5d730018a752ea9d9efb3e1ac0e6e705ac9f7f9863cfa8f612ad43802175338d8d7cc6000000001fc3542434eff03075ea5f0a64f1dfb2f042d281b1a057e9f6c765b533ce51219013ad9484b1e901e62b93e7538f913dcb27695380c3bc579e79f5cc900f28e596e0001431da5f01fe11d58300134caf5ac76e0b1b7486fd02425dd8871bca4afa94d4b01bb39de1c1d10a25ce0cc775bc74b6b0f056c28639e7c5b7651bb8460060085530000000001732ddf661e68c9e335599bb0b18b048d2f1c06b20eabd18239ad2f3cc45fa910014496bab5eedab205b5f2a206bd1db30c5bc8bc0c1914a102f87010f3431be21a0000010b5fd8e7610754075f936463780e85841f3ab8ca2978f9afdf7c2c250f16a75f01db56bc66eb1cd54ec6861e5cf24af2f4a17991556a52ca781007569e95b9842401c03877ecdd98378b321250640a1885604d675aaa50380e49da8cfa6ff7deaf15"
            }
        }}})).unwrap();

    let builder = ZCoinBuilder::new(
        &ctx,
        ticker,
        &conf,
        &activation_params,
        PrivKeyBuildPolicy::detect_priv_key_policy(&ctx).unwrap(),
        PathBuf::new(),
        None,
        protocol_info,
    );

    let z_spending_key = match builder.z_spending_key {
        Some(ref z_spending_key) => z_spending_key.clone(),
        None => extended_spending_key_from_protocol_info_and_policy(
            &builder.protocol_info,
            &builder.priv_key_policy,
            builder.z_coin_params.account,
        )
        .unwrap(),
    };

    WalletDbShared::new(&builder, None, &z_spending_key).await.unwrap()
}

#[cfg(target_arch = "wasm32")]
mod wallet_db_storage_tests {
    use crate::z_coin::storage::walletdb::wallet_db_from_zcoin_builder_for_test;
    use common::log::info;
    use mm2_test_helpers::for_tests::mm_ctx_with_custom_db;
    use wasm_bindgen_test::*;
    use zcash_client_backend::wallet::AccountId;
    use zcash_extras::WalletRead;
    use zcash_primitives::transaction::components::Amount;
    use zcash_primitives::zip32::{ExtendedFullViewingKey, ExtendedSpendingKey};

    wasm_bindgen_test_configure!(run_in_browser);

    const TICKER: &str = "ARRR";

    #[wasm_bindgen_test]
    async fn test_intialize_db_impl() {
        let ctx = mm_ctx_with_custom_db();
        info!("ENTER");
        let walletdb = wallet_db_from_zcoin_builder_for_test(&ctx, TICKER).await;
        let db = walletdb.db;

        // Add an account to the wallet
        let extsk = ExtendedSpendingKey::master(&[]);
        let extfvks = [ExtendedFullViewingKey::from(&extsk)];
        db.init_accounts_table(&extfvks).await.unwrap();

        // The account should be empty
        //        assert_eq!(db.get_balance(AccountId(0)).await.unwrap(), Amount::zero());

        // We can't get an anchor height, as we have not scanned any blocks.
        assert_eq!(db.get_target_and_anchor_heights().await.unwrap(), None);

        // An invalid account has zero balance
        assert!(db.get_address(AccountId(1)).await.is_err());
        //        assert_eq!(db.get_balance(AccountId(0)).await.unwrap(), Amount::zero());
    }
}
