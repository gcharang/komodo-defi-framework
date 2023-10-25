use super::*;
use crate::hd_wallet::{ExtractExtendedPubkey, HDAccount, HDAddress, HDExtractPubkeyError, HDWallet, HDXPubExtractor};
use async_trait::async_trait;
use bip32::DerivationPath;
use crypto::Secp256k1ExtendedPublicKey;
use ethereum_types::{Address, Public};
use std::str::FromStr;

pub type EthHDAddress = HDAddress<Address, Public>;
pub type EthHDAccount = HDAccount<EthHDAddress>;
pub type EthHDWallet = HDWallet<EthHDAccount>;

#[async_trait]
impl ExtractExtendedPubkey for EthCoin {
    type ExtendedPublicKey = Secp256k1ExtendedPublicKey;

    async fn extract_extended_pubkey<XPubExtractor>(
        &self,
        xpub_extractor: Option<XPubExtractor>,
        derivation_path: DerivationPath,
    ) -> MmResult<Self::ExtendedPublicKey, HDExtractPubkeyError>
    where
        XPubExtractor: HDXPubExtractor + Send,
    {
        // Todo: there is a lot of repetition between here and utxo
        match xpub_extractor {
            Some(xpub_extractor) => {
                let trezor_coin = self
                    .trezor_coin
                    .clone()
                    .or_mm_err(|| HDExtractPubkeyError::CoinDoesntSupportTrezor)?;
                let xpub = xpub_extractor.extract_xpub(trezor_coin, derivation_path).await?;
                Secp256k1ExtendedPublicKey::from_str(&xpub)
                    .map_to_mm(|e| HDExtractPubkeyError::InvalidXpub(e.to_string()))
            },
            None => {
                let mut priv_key = self
                    .priv_key_policy
                    .bip39_secp_priv_key_or_err()
                    .mm_err(|e| HDExtractPubkeyError::Internal(e.to_string()))?
                    .clone();
                for child in derivation_path {
                    priv_key = priv_key
                        .derive_child(child)
                        .map_to_mm(|e| HDExtractPubkeyError::Internal(e.to_string()))?;
                }
                drop_mutability!(priv_key);
                Ok(priv_key.public_key())
            },
        }
    }
}

#[async_trait]
impl HDWalletCoinOps for EthCoin {
    type HDWallet = EthHDWallet;

    fn address_from_extended_pubkey(
        &self,
        extended_pubkey: &Secp256k1ExtendedPublicKey,
        derivation_path: DerivationPath,
    ) -> HDCoinHDAddress<Self> {
        let serialized = extended_pubkey.public_key().serialize_uncompressed();
        let mut pubkey = Public::default();
        pubkey.as_mut().copy_from_slice(&serialized[1..65]);
        drop_mutability!(pubkey);

        let address = public_to_address(&pubkey);

        EthHDAddress {
            address,
            pubkey,
            derivation_path,
        }
    }

    fn trezor_coin(&self) -> MmResult<String, NewAddressDeriveConfirmError> {
        self.trezor_coin.clone().or_mm_err(|| {
            // Todo: this can be made common with utxo
            let ticker = self.ticker();
            let error = format!("'{ticker}' coin must contain the 'trezor_coin' field in the coins config");
            NewAddressDeriveConfirmError::DeriveError(NewAddressDerivingError::Internal(error))
        })
    }
}

impl HDCoinWithdrawOps for EthCoin {}

/// The ETH/ERC20 address balance scanner.
pub enum ETHAddressScanner {
    Web3 {
        web3: Web3<Web3Transport>,
        coin_type: EthCoinType,
    },
}

#[async_trait]
#[cfg_attr(test, mockable)]
impl HDAddressBalanceScanner for ETHAddressScanner {
    type Address = Address;

    async fn is_address_used(&self, address: &Self::Address) -> BalanceResult<bool> {
        match self {
            ETHAddressScanner::Web3 { web3, coin_type } => {
                let current_block = match web3.eth().block_number().await {
                    Ok(block) => block,
                    Err(e) => {
                        return Err(BalanceError::Transport(format!("Error {} on eth_block_number", e)).into());
                    },
                };

                let from_block = BlockNumber::Earliest;
                let to_block = BlockNumber::Number(current_block);

                match coin_type {
                    EthCoinType::Eth => {
                        // It makes sense to check transactions to the hd address first since an address
                        // should have incoming transactions before making any outgoing ones, so this will
                        // avoid an additional call in almost all cases
                        let to_traces = eth_traces(web3, vec![], vec![*address], from_block, to_block, Some(1)).await?;

                        if !to_traces.is_empty() {
                            return Ok(true);
                        }

                        let from_traces =
                            eth_traces(web3, vec![*address], vec![], from_block, to_block, Some(1)).await?;

                        Ok(!from_traces.is_empty())
                    },
                    EthCoinType::Erc20 { token_addr, .. } => {
                        // It makes sense to check transactions to the hd address first since an address
                        // should have incoming transactions before making any outgoing ones, so this will
                        // avoid an additional call in almost all cases
                        let to_events = erc20_transfer_events(
                            web3,
                            *token_addr,
                            None,
                            Some(*address),
                            from_block,
                            to_block,
                            Some(1),
                        )
                        .await?;

                        if !to_events.is_empty() {
                            return Ok(true);
                        }

                        let from_events = erc20_transfer_events(
                            web3,
                            *token_addr,
                            Some(*address),
                            None,
                            from_block,
                            to_block,
                            Some(1),
                        )
                        .await?;

                        Ok(!from_events.is_empty())
                    },
                }
            },
        }
    }
}

#[async_trait]
impl HDWalletBalanceOps for EthCoin {
    type HDAddressScanner = ETHAddressScanner;

    async fn produce_hd_address_scanner(&self) -> BalanceResult<Self::HDAddressScanner> {
        Ok(ETHAddressScanner::Web3 {
            web3: self.web3.clone(),
            coin_type: self.coin_type.clone(),
        })
    }

    async fn enable_hd_wallet<XPubExtractor>(
        &self,
        hd_wallet: &Self::HDWallet,
        xpub_extractor: Option<XPubExtractor>,
        params: EnabledCoinBalanceParams,
        path_to_address: &HDAccountAddressId,
    ) -> MmResult<HDWalletBalance, EnableCoinBalanceError>
    where
        XPubExtractor: HDXPubExtractor + Send,
    {
        coin_balance::common_impl::enable_hd_wallet(self, hd_wallet, xpub_extractor, params, path_to_address).await
    }

    async fn scan_for_new_addresses(
        &self,
        hd_wallet: &Self::HDWallet,
        hd_account: &mut HDCoinHDAccount<Self>,
        address_scanner: &Self::HDAddressScanner,
        gap_limit: u32,
    ) -> BalanceResult<Vec<HDAddressBalance>> {
        scan_for_new_addresses_impl(
            self,
            hd_wallet,
            hd_account,
            address_scanner,
            Bip44Chain::External,
            gap_limit,
        )
        .await
    }

    async fn all_known_addresses_balances(
        &self,
        hd_account: &HDCoinHDAccount<Self>,
    ) -> BalanceResult<Vec<HDAddressBalance>> {
        let external_addresses = hd_account
            .known_addresses_number(Bip44Chain::External)
            // A UTXO coin should support both [`Bip44Chain::External`] and [`Bip44Chain::Internal`].
            .mm_err(|e| BalanceError::Internal(e.to_string()))?;

        self.known_addresses_balances_with_ids(hd_account, Bip44Chain::External, 0..external_addresses)
            .await
    }

    async fn known_address_balance(&self, address: &HDBalanceAddress<Self>) -> BalanceResult<CoinBalance> {
        let balance = self
            .address_balance(*address)
            .and_then(move |result| Ok(u256_to_big_decimal(result, self.decimals())?))
            .compat()
            .await?;

        Ok(CoinBalance {
            spendable: balance,
            unspendable: BigDecimal::from(0),
        })
    }

    async fn known_addresses_balances(
        &self,
        addresses: Vec<HDBalanceAddress<Self>>,
    ) -> BalanceResult<Vec<(HDBalanceAddress<Self>, CoinBalance)>> {
        // Todo: check how it's done for utxo, we should make this concurrent call
        let mut balances = vec![];
        for address in addresses {
            let balance = self.known_address_balance(&address).await?;
            balances.push((address, balance));
        }
        Ok(balances)
    }
}
