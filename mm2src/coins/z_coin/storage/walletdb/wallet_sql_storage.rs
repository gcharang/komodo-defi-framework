use crate::z_coin::storage::{WalletDbError, WalletDbShared};
use crate::z_coin::z_rpc::create_wallet_db;
use crate::z_coin::{extended_spending_key_from_protocol_info_and_policy, ZCoinBuilder};
use mm2_err_handle::prelude::{MmError, MmResult};
use zcash_primitives::zip32::ExtendedFullViewingKey;

impl<'a> WalletDbShared {
    pub async fn new(zcoin_builder: &ZCoinBuilder<'a>) -> MmResult<Self, WalletDbError> {
        let z_spending_key = match zcoin_builder.z_spending_key {
            Some(ref z_spending_key) => z_spending_key.clone(),
            None => extended_spending_key_from_protocol_info_and_policy(
                &zcoin_builder.protocol_info,
                &zcoin_builder.priv_key_policy,
            )
            .map_err(|err| WalletDbError::ZCoinBuildError(err.to_string()))?,
        };
        let wallet_db = create_wallet_db(
            zcoin_builder
                .db_dir_path
                .join(format!("{}_wallet.db", zcoin_builder.ticker)),
            zcoin_builder.protocol_info.consensus_params.clone(),
            zcoin_builder.protocol_info.check_point_block.clone(),
            ExtendedFullViewingKey::from(&z_spending_key),
        )
        .await
        .map_err(|err| MmError::new(WalletDbError::ZcoinClientInitError(err.into_inner())))?;

        Ok(Self {
            db: wallet_db,
            ticker: zcoin_builder.ticker.to_string(),
        })
    }
}
