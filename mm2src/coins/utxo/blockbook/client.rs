use crate::utxo::blockbook::structures::{BalanceHistoryParams, BlockBookAddress, BlockBookBalanceHistory,
                                         BlockBookBlock, BlockBookTickers, BlockBookTickersList, BlockBookTransaction,
                                         BlockBookTransactionSpecific, BlockBookUtxo, BlookBookStatus,
                                         GetAddressParams, GetBlockByHashHeight, XpubTransactions};
use crate::utxo::rpc_clients::{BlockHashOrHeight, EstimateFeeMethod, EstimateFeeMode, JsonRpcPendingRequestsShared,
                               SpentOutputInfo, UnspentInfo, UnspentMap, UtxoClientError, UtxoClientFut,
                               UtxoClientOps, UtxoJsonRpcClientInfo};
use crate::utxo::utxo_block_header_storage::BlockHeaderStorage;
use crate::utxo::{GetBlockHeaderError, NonZeroU64, UtxoTx};
use crate::{RpcTransportEventHandler, RpcTransportEventHandlerShared};
use async_trait::async_trait;
use chain::TransactionInput;
use common::executor::abortable_queue::AbortableQueue;
use common::jsonrpc_client::{JsonRpcClient, JsonRpcErrorType, JsonRpcRemoteAddr, JsonRpcRequest, JsonRpcRequestEnum,
                             JsonRpcResponseEnum, JsonRpcResponseFut, RpcRes};
use common::{block_on, APPLICATION_JSON};
use futures::FutureExt;
use futures::TryFutureExt;
use futures01::sync::mpsc;
use futures01::{Future, Stream};
use http::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use http::{Request, StatusCode};
use keys::Address;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::mm_error::MmError;
use mm2_err_handle::prelude::{MapMmError, MapToMmFutureExt, MapToMmResult, MmResult};
#[cfg(not(target_arch = "wasm32"))]
use mm2_net::transport::slurp_req_body;
use mm2_number::BigDecimal;
use rpc::v1::types::{deserialize_null_default, Bytes, CoinbaseTransactionInput, RawTransaction,
                     SignedTransactionOutput, Transaction, TransactionInputEnum, TransactionOutputScript, H256};
use serde::{Deserialize, Deserializer};
use serde_json::{self as json, Value as Json};
use serialization::CoinVariant;
use std::fmt::Display;
use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))] use tonic::IntoRequest;

const BTC_BLOCKBOOK_ENPOINT: &str = "https://btc1.trezor.io";

pub type BlockBookResult<T> = MmResult<T, BlockBookClientError>;

#[derive(Debug, Clone)]
pub struct BlockBookClientImpl {
    pub ticker: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct BlockBookClient(pub Arc<BlockBookClientImpl>);

impl BlockBookClient {
    pub fn new(url: &str, ticker: &str) -> Self {
        Self(Arc::new(BlockBookClientImpl {
            url: url.to_string(),
            ticker: ticker.to_string(),
        }))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn query(&self, path: String) -> BlockBookResult<Json> {
        use http::header::HeaderValue;

        let uri = format!("{}{path}", self.0.url);
        let request = http::Request::builder()
            .method("GET")
            .uri(uri.clone())
            .header(ACCEPT, HeaderValue::from_static(APPLICATION_JSON))
            .header(
                USER_AGENT,
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:90.0) Gecko/20100101 Firefox/90.0",
            )
            .body(hyper::Body::from(""))
            .map_err(|err| BlockBookClientError::Transport(err.to_string()))?;

        let (status, _header, body) = slurp_req_body(request)
            .await
            .map_err(|err| MmError::new(BlockBookClientError::Transport(err.to_string())))?;

        if !status.is_success() {
            return Err(MmError::new(BlockBookClientError::Transport(format!(
                "Response !200 from {}: {}, {}",
                uri, status, body
            ))));
        }

        Ok(body)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn query(&self, path: String) -> BlockBookResult<Json> { todo!() }

    /// Status page returns current status of Blockbook and connected backend.
    pub async fn status(&self) -> BlockBookResult<BlookBookStatus> {
        let res = self.query("/api/v2".to_string()).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get current block of a given height.
    pub async fn get_block_hash(&self, height: u64) -> BlockBookResult<H256> {
        let path = format!("/api/v2/block-index/{height}");
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res["blockHash"].clone())
            .map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get transaction returns "normalized" data about transaction, which has the same general structure for all
    /// supported coins. It does not return coin specific fields.
    pub async fn get_transaction(&self, txid: &str) -> BlockBookResult<BlockBookTransaction> {
        let path = format!("/api/v2/tx/{txid}");
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get transaction data in the exact format as returned by backend, including all coin specific fields:
    pub async fn get_transaction_specific(&self, txid: &str) -> BlockBookResult<BlockBookTransactionSpecific> {
        let path = format!("/api/v2/tx-specific/{txid}");
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get balances and transactions of an address. The returned transactions are sorted by block height, newest
    /// blocks first. see `[strutures::GetAddressParams]` for extra query arguments.
    pub async fn get_address(
        &self,
        address: &str,
        _query_params: Option<GetAddressParams>,
    ) -> BlockBookResult<BlockBookAddress> {
        let path = format!("/api/v2/address/{address}");
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get balances and transactions of an xpub or output descriptor, applicable only for Bitcoin-type coins. see
    /// `[strutures::GetAddressParams]` for extra query arguments.
    pub async fn get_xpub(
        &self,
        xpub: &str,
        _query_params: Option<GetAddressParams>,
    ) -> BlockBookResult<XpubTransactions> {
        let path = format!("/api/v2/xpub/{xpub}");
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    // Get array of unspent transaction outputs of address or xpub, applicable only for Bitcoin-type coins. By default,
    // the list contains both confirmed and unconfirmed transactions. The query parameter confirmed=true disables return of unconfirmed transactions. The returned utxos are sorted by block height, newest blocks first. For xpubs or output descriptors, the response also contains address and derivation path of the utxo.
    pub async fn get_utxo(&self, address: &str, _confirmed: bool) -> BlockBookResult<Vec<BlockBookUtxo>> {
        let path = format!("/api/v2/utxo/{address}");
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get information about block with transactions, either by height or hash
    pub async fn get_block<T: Clone + Display>(
        &self,
        block_by: GetBlockByHashHeight<T>,
    ) -> BlockBookResult<BlockBookBlock> {
        let path = format!("/api/v2/block/{}", block_by.get_inner());
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Sends new transaction to backend.
    pub async fn send_transaction(&self, hex: &RawTransaction) -> BlockBookResult<H256> {
        let path = format!("/api/v2/sendtx/{:?}", hex);
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get a list of available currency rate tickers (secondary currencies) for the specified date, along with an
    /// actual data timestamp.
    pub async fn get_tickers_list(&self, timestamp: Option<u32>) -> BlockBookResult<BlockBookTickersList> {
        let timestamp_query = timestamp.map(|ts| format!("?timestamp={}", ts)).unwrap_or_default();
        let path = format!("/api/v2/tickers-list/{}", timestamp_query);
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Get currency rate for the specified currency and date. If the currency is not available for that specific
    /// timestamp, the next closest rate will be returned. All responses contain an actual rate timestamp.
    pub async fn get_tickers(
        &self,
        currency: Option<&str>,
        timestamp: Option<u32>,
    ) -> BlockBookResult<BlockBookTickers> {
        let timestamp_query = timestamp.map(|ts| format!("timestamp={}", ts));
        let currency_query = currency.map(|cur| format!("currency={}", cur));

        let path = match (timestamp_query, currency_query) {
            (Some(ts), Some(cur)) => format!("/api/v2/tickers/?{}&{}", ts, cur),
            (Some(ts), None) => format!("/api/v2/tickers/?{}", ts),
            (None, Some(cur)) => format!("/api/v2/tickers/?{}", cur),
            (None, None) => "/api/v2/tickers/".to_string(),
        };

        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }

    /// Returns a balance history for the specified XPUB or address.
    pub async fn balance_history(
        &self,
        address: &str,
        _query_params: BalanceHistoryParams,
    ) -> BlockBookResult<BlockBookBalanceHistory> {
        let path = format!("/api/v2/balancehistory/{address}");
        let res = self.query(path).await?;
        Ok(serde_json::from_value(res).map_err(|err| BlockBookClientError::ResponseError(err.to_string()))?)
    }
}

#[async_trait]
impl UtxoClientOps for BlockBookClient {
    fn list_unspent(&self, _address: &Address, _decimals: u8) -> UtxoClientFut<Vec<UnspentInfo>> { todo!() }

    fn list_unspent_group(&self, _addresses: Vec<Address>, _decimals: u8) -> UtxoClientFut<UnspentMap> { todo!() }

    fn send_transaction(&self, _tx: &UtxoTx) -> UtxoClientFut<H256> { todo!() }

    fn send_raw_transaction(&self, _tx: Bytes) -> UtxoClientFut<H256> { todo!() }

    fn get_transaction_bytes(&self, txid: &H256) -> UtxoClientFut<Bytes> {
        let selfi = self.clone();
        let txid_clone = *txid;
        let fut = async move {
            let tx = selfi
                .get_transaction_specific(&txid_clone.to_string())
                .await
                .map(|res| res.hex)
                .mm_err(|err| UtxoClientError::Internal(err.to_string()));

            tx
        };

        Box::new(fut.boxed().compat())
    }

    fn get_verbose_transaction(&self, txid: &H256) -> UtxoClientFut<Transaction> {
        let selfi = self.clone();
        let txid_clone = *txid;
        let fut = async move {
            let tx = selfi
                .get_transaction_specific(&txid_clone.to_string())
                .await
                .map(Transaction::from)
                .mm_err(|err| UtxoClientError::Internal(err.to_string()));

            tx
        };

        Box::new(fut.boxed().compat())
    }

    fn get_verbose_transactions(&self, tx_ids: &[H256]) -> UtxoClientFut<Vec<Transaction>> {
        let selfi = self.clone();
        let tx_ids_clone = tx_ids.to_owned();
        let fut = async move {
            let mut verbose_transactions = Vec::new();
            for txid in tx_ids_clone {
                let tx = selfi
                    .get_transaction_specific(&txid.to_string())
                    .await
                    .map(Transaction::from)
                    .map_err(|err| UtxoClientError::Internal(err.to_string()))?;
                verbose_transactions.push(tx);
            }
            Ok(verbose_transactions)
        };

        Box::new(fut.boxed().compat())
    }

    fn get_block_count(&self) -> UtxoClientFut<u64> { todo!() }

    fn display_balance(&self, _address: Address, _decimals: u8) -> RpcRes<BigDecimal> { todo!() }

    fn display_balances(&self, _addresses: Vec<Address>, _decimals: u8) -> UtxoClientFut<Vec<(Address, BigDecimal)>> {
        todo!()
    }

    fn estimate_fee_sat(
        &self,
        _decimals: u8,
        _fee_method: &EstimateFeeMethod,
        _mode: &Option<EstimateFeeMode>,
        _n_blocks: u32,
    ) -> UtxoClientFut<u64> {
        todo!()
    }

    fn get_relay_fee(&self) -> RpcRes<BigDecimal> { todo!() }

    fn find_output_spend(
        &self,
        _tx_hash: primitives::hash::H256,
        _script_pubkey: &[u8],
        _vout: usize,
        _from_block: BlockHashOrHeight,
    ) -> Box<dyn Future<Item = Option<SpentOutputInfo>, Error = String> + Send> {
        todo!()
    }

    fn get_median_time_past(
        &self,
        _starting_block: u64,
        _count: NonZeroU64,
        _coin_variant: CoinVariant,
    ) -> UtxoClientFut<u32> {
        todo!()
    }

    async fn get_block_timestamp(&self, _height: u64) -> Result<u64, MmError<GetBlockHeaderError>> { todo!() }
}

#[derive(Debug, Display)]
pub enum BlockBookClientError {
    Transport(String),
    ResponseError(String),
    #[display(fmt = "'{}' asset is not yet supported by blockbook", coin)]
    NotSupported {
        coin: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc::v1::types::ScriptType::{NullData, ScriptHash};
    use rpc::v1::types::TransactionInputEnum::Coinbase;

    lazy_static! {
        pub static ref BLOCKBOOK: BlockBookClient = BlockBookClient::new(BTC_BLOCKBOOK_ENPOINT, "BTC");
    }

    #[test]
    fn test_status() {
        let status = block_on(BLOCKBOOK.status()).unwrap();

        assert_eq!("Bitcoin", status.blockbook.coin)
    }

    #[test]
    fn test_block_hash() {
        let block_hash = block_on(BLOCKBOOK.get_block_hash(0)).unwrap();
        let genesis_block = H256::from_str("000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f").unwrap();

        assert_eq!(genesis_block, block_hash);
    }

    #[test]
    fn test_get_verbose_transaction() {
        let transaction = BLOCKBOOK
            .get_verbose_transaction(&H256::from(
                "bdb31013359ff66978e7a8bba987ba718a556c85c4051ddb1e83b1b36860734b",
            ))
            .wait()
            .unwrap();

        let expected_tx = Transaction {
            hex: Bytes(vec![
                1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 255, 255, 255, 255, 80, 3, 32, 226, 11, 19, 98, 105, 110, 97, 110, 99, 101, 47, 56, 48,
                55, 178, 0, 48, 0, 35, 147, 150, 8, 250, 190, 109, 109, 85, 246, 160, 219, 230, 155, 23, 161, 229, 13,
                137, 107, 38, 16, 16, 102, 227, 14, 26, 226, 84, 95, 222, 114, 228, 115, 87, 134, 57, 166, 219, 20, 4,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 205, 134, 50, 142, 29, 0, 0, 0, 0, 0, 255, 255, 255, 255, 3, 210, 124, 195,
                37, 0, 0, 0, 0, 23, 169, 20, 202, 53, 177, 244, 208, 41, 7, 49, 72, 82, 240, 153, 53, 185, 96, 69, 7,
                248, 215, 0, 135, 0, 0, 0, 0, 0, 0, 0, 0, 38, 106, 36, 170, 33, 169, 237, 221, 103, 104, 188, 22, 61,
                130, 222, 19, 18, 194, 188, 165, 199, 134, 34, 252, 224, 38, 69, 251, 200, 242, 113, 177, 22, 24, 212,
                103, 41, 103, 117, 0, 0, 0, 0, 0, 0, 0, 0, 43, 106, 41, 82, 83, 75, 66, 76, 79, 67, 75, 58, 67, 117,
                154, 168, 149, 76, 65, 156, 1, 144, 79, 219, 132, 54, 75, 40, 241, 84, 76, 217, 31, 158, 196, 177, 90,
                39, 117, 36, 0, 77, 184, 95, 1, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]),
            txid: H256::from_str("bdb31013359ff66978e7a8bba987ba718a556c85c4051ddb1e83b1b36860734b").unwrap(),
            hash: None,
            size: Some(298),
            vsize: Some(271),
            version: 1,
            locktime: 0,
            vin: vec![Coinbase(CoinbaseTransactionInput {
                coinbase: Bytes(vec![
                    3, 32, 226, 11, 19, 98, 105, 110, 97, 110, 99, 101, 47, 56, 48, 55, 178, 0, 48, 0, 35, 147, 150, 8,
                    250, 190, 109, 109, 85, 246, 160, 219, 230, 155, 23, 161, 229, 13, 137, 107, 38, 16, 16, 102, 227,
                    14, 26, 226, 84, 95, 222, 114, 228, 115, 87, 134, 57, 166, 219, 20, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    205, 134, 50, 142, 29, 0, 0, 0, 0, 0,
                ]),
                sequence: 4294967295,
            })],
            vout: vec![
                SignedTransactionOutput {
                    value: Some(6.33568466),
                    n: 0,
                    script: TransactionOutputScript {
                        asm: "OP_HASH160 \
            ca35b1f4d02907314852f09935b9604507f8d700 OP_EQUAL"
                            .to_string(),
                        hex: Bytes(vec![
                            169, 20, 202, 53, 177, 244, 208, 41, 7, 49, 72, 82, 240, 153, 53, 185, 96, 69, 7, 248, 215,
                            0, 135,
                        ]),
                        req_sigs: 0,
                        script_type: ScriptHash,
                        addresses: vec![],
                    },
                },
                SignedTransactionOutput {
                    value: Some(0.0),
                    n: 1,
                    script: TransactionOutputScript {
                        asm: "OP_RETURN \
        aa21a9eddd6768bc163d82de1312c2bca5c78622fce02645fbc8f271b11618d467296775"
                            .to_string(),
                        hex: Bytes(vec![
                            106, 36, 170, 33, 169, 237, 221, 103, 104, 188, 22, 61, 130, 222, 19, 18, 194, 188, 165,
                            199, 134, 34, 252, 224, 38, 69, 251, 200, 242, 113, 177, 22, 24, 212, 103, 41, 103, 117,
                        ]),
                        req_sigs: 0,
                        script_type: NullData,
                        addresses: vec![],
                    },
                },
                SignedTransactionOutput {
                    value: Some(0.0),
                    n: 2,
                    script: TransactionOutputScript {
                        asm: "OP_RETURN \
        52534b424c4f434b3a43759aa8954c419c01904fdb84364b28f1544cd91f9ec4b15a277524004db85f"
                            .to_string(),
                        hex: Bytes(vec![
                            106, 41, 82, 83, 75, 66, 76, 79, 67, 75, 58, 67, 117, 154, 168, 149, 76, 65, 156, 1, 144,
                            79, 219, 132, 54, 75, 40, 241, 84, 76, 217, 31, 158, 196, 177, 90, 39, 117, 36, 0, 77, 184,
                            95,
                        ]),
                        req_sigs: 0,
                        script_type: NullData,
                        addresses: vec![],
                    },
                },
            ],
            blockhash: H256::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
            confirmations: 18862,
            rawconfirmations: None,
            time: 1677663144,
            blocktime: 1677663144,
            height: None,
        };

        //        assert_eq!(expected_tx, transaction)
    }
}
