use super::rpc_clients::{UtxoJsonRpcClientInfo, UtxoRpcClientOps};
use crate::utxo::NonZeroU64;
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct BlockBookClient(pub Arc<BlockBookClient>);

#[derive(Clone, Debug)]
pub struct BlockBookClientImpl {
    uri: String,
    block_headers: String,
    ticker: String,
}

#[async_trait]
impl UtxoJsonRpcClientInfo for BlockBookClientImpl {
    fn coin_name(&self) -> &str { &self.coin_name() }

    fn client_info(&self) -> String { format!("coin: {}", self.coin_name()) }
}

impl UtxoRpcClientOps for BlockBookClientImpl {
    fn list_unspent(
        &self,
        address: &keys::Address,
        decimals: u8,
    ) -> super::rpc_clients::UtxoRpcFut<Vec<super::rpc_clients::UnspentInfo>> {
        todo!()
    }

    fn list_unspent_group(
        &self,
        addresses: Vec<keys::Address>,
        decimals: u8,
    ) -> super::rpc_clients::UtxoRpcFut<super::rpc_clients::UnspentMap> {
        todo!()
    }

    fn send_transaction(&self, tx: &chain::Transaction) -> super::rpc_clients::UtxoRpcFut<rpc::v1::types::H256> {
        todo!()
    }

    fn send_raw_transaction(&self, tx: rpc::v1::types::Bytes) -> super::rpc_clients::UtxoRpcFut<rpc::v1::types::H256> {
        todo!()
    }

    fn get_transaction_bytes(
        &self,
        txid: &rpc::v1::types::H256,
    ) -> super::rpc_clients::UtxoRpcFut<rpc::v1::types::Bytes> {
        todo!()
    }

    fn get_verbose_transaction(
        &self,
        txid: &rpc::v1::types::H256,
    ) -> super::rpc_clients::UtxoRpcFut<rpc::v1::types::Transaction> {
        todo!()
    }

    fn get_verbose_transactions(
        &self,
        tx_ids: &[rpc::v1::types::H256],
    ) -> super::rpc_clients::UtxoRpcFut<Vec<rpc::v1::types::Transaction>> {
        todo!()
    }

    fn get_block_count(&self) -> super::rpc_clients::UtxoRpcFut<u64> { todo!() }

    fn display_balance(
        &self,
        address: keys::Address,
        decimals: u8,
    ) -> common::jsonrpc_client::RpcRes<mm2_number::BigDecimal> {
        todo!()
    }

    fn display_balances(
        &self,
        addresses: Vec<keys::Address>,
        decimals: u8,
    ) -> super::rpc_clients::UtxoRpcFut<Vec<(keys::Address, mm2_number::BigDecimal)>> {
        todo!()
    }

    fn estimate_fee_sat(
        &self,
        decimals: u8,
        fee_method: &super::rpc_clients::EstimateFeeMethod,
        mode: &Option<super::rpc_clients::EstimateFeeMode>,
        n_blocks: u32,
    ) -> super::rpc_clients::UtxoRpcFut<u64> {
        todo!()
    }

    fn get_relay_fee(&self) -> common::jsonrpc_client::RpcRes<mm2_number::BigDecimal> { todo!() }

    fn find_output_spend(
        &self,
        tx_hash: primitives::hash::H256,
        script_pubkey: &[u8],
        vout: usize,
        from_block: super::rpc_clients::BlockHashOrHeight,
    ) -> Box<dyn futures01::Future<Item = Option<super::rpc_clients::SpentOutputInfo>, Error = String> + Send> {
        todo!()
    }

    fn get_median_time_past(
        &self,
        starting_block: u64,
        count: NonZeroU64,
        coin_variant: serialization::CoinVariant,
    ) -> super::rpc_clients::UtxoRpcFut<u32> {
        todo!()
    }

    async fn get_block_timestamp(
        &self,
        height: u64,
    ) -> Result<u64, mm2_err_handle::prelude::MmError<super::GetBlockHeaderError>> {
        todo!()
    }
}
