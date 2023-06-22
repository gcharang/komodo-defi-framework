use crate::utxo::rpc_clients::{BlockHashOrHeight, EstimateFeeMethod, EstimateFeeMode, SpentOutputInfo, UnspentInfo,
                               UnspentMap, UtxoJsonRpcClientInfo, UtxoRpcClientOps, UtxoRpcFut};
use crate::utxo::utxo_block_header_storage::BlockHeaderStorage;
use crate::utxo::{GetBlockHeaderError, NonZeroU64};
use async_trait::async_trait;
use common::jsonrpc_client::{JsonRpcClient, JsonRpcRequestEnum, JsonRpcResponseFut, RpcRes};
use futures01::Future;
use keys::Address;
use mm2_err_handle::mm_error::MmError;
use mm2_number::BigDecimal;
use rpc::v1::types::{Bytes, Transaction, H256};
use serialization::CoinVariant;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct BlockBookClient(pub Arc<BlockBookClient>);

#[derive(Debug)]
pub struct BlockBookClientImpl {
    uri: String,
    coin_name: String,
    block_headers_storage: BlockHeaderStorage,
    next_id: AtomicU64,
}

impl JsonRpcClient for BlockBookClientImpl {
    fn version(&self) -> &'static str { "1" }

    fn next_id(&self) -> String { self.next_id.fetch_add(1, Relaxed).to_string() }

    fn client_info(&self) -> String { UtxoJsonRpcClientInfo::client_info(self) }

    fn transport(&self, request: JsonRpcRequestEnum) -> JsonRpcResponseFut { todo!() }
}

impl UtxoJsonRpcClientInfo for BlockBookClientImpl {
    fn coin_name(&self) -> &str { self.coin_name.as_str() }
}

#[async_trait]
impl UtxoRpcClientOps for BlockBookClientImpl {
    fn list_unspent(&self, address: &Address, decimals: u8) -> UtxoRpcFut<Vec<UnspentInfo>> { todo!() }

    fn list_unspent_group(&self, addresses: Vec<Address>, decimals: u8) -> UtxoRpcFut<UnspentMap> { todo!() }

    fn send_transaction(&self, tx: &chain::Transaction) -> UtxoRpcFut<H256> { todo!() }

    fn send_raw_transaction(&self, tx: Bytes) -> UtxoRpcFut<H256> { todo!() }

    fn get_transaction_bytes(&self, txid: &H256) -> UtxoRpcFut<Bytes> { todo!() }

    fn get_verbose_transaction(&self, txid: &H256) -> UtxoRpcFut<Transaction> { todo!() }

    fn get_verbose_transactions(&self, tx_ids: &[H256]) -> UtxoRpcFut<Vec<Transaction>> { todo!() }

    fn get_block_count(&self) -> UtxoRpcFut<u64> { todo!() }

    fn display_balance(&self, address: Address, decimals: u8) -> RpcRes<BigDecimal> { todo!() }

    fn display_balances(&self, addresses: Vec<Address>, decimals: u8) -> UtxoRpcFut<Vec<(Address, BigDecimal)>> {
        todo!()
    }

    fn estimate_fee_sat(
        &self,
        decimals: u8,
        fee_method: &EstimateFeeMethod,
        mode: &Option<EstimateFeeMode>,
        n_blocks: u32,
    ) -> UtxoRpcFut<u64> {
        todo!()
    }

    fn get_relay_fee(&self) -> RpcRes<BigDecimal> { todo!() }

    fn find_output_spend(
        &self,
        tx_hash: keys::hash::H256,
        script_pubkey: &[u8],
        vout: usize,
        from_block: BlockHashOrHeight,
    ) -> Box<dyn Future<Item = Option<SpentOutputInfo>, Error = String> + Send> {
        todo!()
    }

    fn get_median_time_past(
        &self,
        starting_block: u64,
        count: NonZeroU64,
        coin_variant: CoinVariant,
    ) -> UtxoRpcFut<u32> {
        todo!()
    }

    async fn get_block_timestamp(&self, height: u64) -> Result<u64, MmError<GetBlockHeaderError>> { todo!() }
}
