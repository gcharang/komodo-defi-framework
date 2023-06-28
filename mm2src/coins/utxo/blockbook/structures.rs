use super::serde_helper::{deserialize_amount, deserialize_hex_string};
use crate::utxo::utxo_block_header_storage::BlockHeaderStorage;
use crate::utxo::{GetBlockHeaderError, NonZeroU64};
use bitcoin::Amount;
use bitcoin::Denomination::Satoshi;
use chain::TransactionInput;
use keys::Address;
use mm2_number::BigDecimal;
use rpc::v1::types::{deserialize_null_default, Bytes, RawTransaction, SignedTransactionOutput, TransactionInputEnum,
                     TransactionOutputScript, H256};
use serde::{Deserialize, Deserializer};
use serde_json::{self as json, Value as Json};
use serialization::CoinVariant;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Signed transaction output
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BlockBookTransactionOutput {
    /// Output value in BTC
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_amount")]
    pub value: Option<f64>,
    /// Output index
    pub n: u32,
    pub hex: RawTransaction,
    pub addresses: Option<Vec<String>>,
    #[serde(rename = "isAddress")]
    pub is_address: bool,
    #[serde(rename = "spentTxId")]
    pub spent_txid: Option<H256>,
    #[serde(rename = "spentIndex")]
    pub spent_index: Option<usize>,
    #[serde(rename = "spentHeight")]
    pub spent_height: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct BlockBookTransaction {
    /// Raw transaction
    pub hex: RawTransaction,
    /// The transaction id (same as provided)
    pub txid: H256,
    /// The version
    pub version: i32,
    /// Hash of the block this transaction is included in
    #[serde(default)]
    #[serde(rename = "blockHash")]
    pub block_hash: H256,
    /// The block time in seconds since epoch (Jan 1 1970 GMT)
    #[serde(rename = "blockTime")]
    pub block_time: u32,
    /// The block height transaction mined in
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "blockHeight")]
    pub height: Option<u64>,
    /// The serialized transaction size
    pub size: Option<usize>,
    /// The virtual transaction size (differs from size for witness transactions)
    pub vsize: Option<usize>,
    /// Transaction inputs
    pub vin: Vec<TransactionInputEnum>,
    /// Transaction outputs
    pub vout: Vec<BlockBookTransactionOutput>,
    /// Number of confirmations of this transaction
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_null_default")]
    pub confirmations: u32,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_null_default")]
    #[serde(rename = "confirmationETABlocks")]
    pub confirmations_eta_blocks: u32,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_null_default")]
    #[serde(rename = "confirmationETASeconds")]
    pub confirmations_eta_seconds: u32,
    #[serde(deserialize_with = "deserialize_amount")]
    pub value: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "valueIn")]
    pub value_in: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    pub fees: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct VShieldedSpend {
    pub cv: H256,
    pub anchor: H256,
    pub nullifier: H256,
    pub rk: H256,
    pub proof: H256,
    #[serde(rename = "spendAuthSig")]
    pub spend_auth_sig: Bytes,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct VShieldedSpendOutput {
    pub cv: H256,
    pub cmu: H256,
    #[serde(rename = "ephemeralKey")]
    pub ephemeral_key: H256,
    #[serde(rename = "encCiphertext")]
    pub enc_cipher_text: Bytes,
    #[serde(rename = "outCiphertext")]
    pub out_cipher_text: Bytes,
    #[serde(rename = "spendAuthSig")]
    pub spend_auth_sig: Bytes,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct BlockBookTransactionSpecific {
    /// Raw transaction
    pub hex: RawTransaction,
    /// The transaction id (same as provided)
    pub txid: H256,
    /// The version
    pub version: i32,
    pub overwintered: bool,
    #[serde(rename = "versiongroupid")]
    pub version_group_id: String,
    #[serde(rename = "locktime")]
    pub lock_time: u64,
    #[serde(rename = "expiryHeight")]
    pub expiry_height: u64,
    pub v_join_split: Vec<String>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "valueBalance")]
    pub value_balance: Option<f64>,
    pub v_shielded_spend: Vec<String>,
    /// Hash of the block this transaction is included in
    #[serde(default)]
    #[serde(rename = "blockHash")]
    pub block_hash: H256,
    /// The block time in seconds since epoch (Jan 1 1970 GMT)
    #[serde(rename = "blocktime")]
    pub block_time: u32,
    pub time: u32,
    /// The block height transaction mined in
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "blockHeight")]
    pub height: Option<u64>,
    /// The serialized transaction size
    pub size: Option<usize>,
    /// The virtual transaction size (differs from size for witness transactions)
    pub vsize: Option<usize>,
    /// Transaction inputs
    pub vin: Vec<TransactionInputEnum>,
    /// Transaction outputs
    pub vout: Vec<BlockBookTransactionOutput>,
    /// Number of confirmations of this transaction
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_null_default")]
    pub confirmations: u32,
    #[serde(rename = "bindingSig")]
    pub binding_sig: Bytes,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct BlockBookAddress {
    pub page: u32,
    #[serde(rename = "totalPages")]
    pub total_pages: u32,
    #[serde(rename = "itemsOnPge")]
    pub items_on_page: u32,
    pub address: String,
    #[serde(deserialize_with = "deserialize_amount")]
    pub balance: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "totalReceived")]
    pub total_received: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "totalSent")]
    pub total_sent: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "unconfirmedBalance")]
    pub unconfirmed_balance: Option<f64>,
    #[serde(rename = "unconfirmedTxs")]
    pub unconfirmed_txs: u32,
    pub txs: u32,
    pub txids: Vec<H256>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct Tokens {
    #[serde(rename = "type")]
    pub token_type: String,
    pub name: String,
    pub path: String,
    pub decimals: u8,
    pub transfers: u32,
    #[serde(deserialize_with = "deserialize_amount")]
    pub balance: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "totalReceived")]
    pub total_received: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "totalSent")]
    pub total_sent: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct XpubTransactions {
    pub page: u32,
    #[serde(rename = "totalPages")]
    pub total_pages: u32,
    #[serde(rename = "itemsOnPge")]
    pub items_on_page: u32,
    pub address: String,
    #[serde(deserialize_with = "deserialize_amount")]
    pub balance: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "totalReceived")]
    pub total_received: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "totalSent")]
    pub total_sent: Option<f64>,
    #[serde(deserialize_with = "deserialize_amount")]
    #[serde(rename = "unconfirmedBalance")]
    pub unconfirmed_balance: Option<f64>,
    #[serde(rename = "unconfirmedTxs")]
    pub unconfirmed_txs: u32,
    pub txs: u32,
    pub txids: Vec<H256>,
    #[serde(rename = "usedTokens")]
    pub used_tokens: u16,
    #[serde(default)]
    pub tokens: Vec<Tokens>,
    #[serde(default)]
    pub secondary_value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct BlockBookUtxo {
    pub txid: H256,
    pub vout: u32,
    #[serde(deserialize_with = "deserialize_amount")]
    pub value: Option<f64>,
    pub confirmations: u32,
    #[serde(rename = "locktime")]
    pub lock_time: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct BlockBookBlock {
    pub page: u32,
    #[serde(rename = "totalPages")]
    pub total_pages: u32,
    #[serde(rename = "itemsOnPge")]
    pub items_on_page: u32,
    #[serde(default)]
    #[serde(rename = "blockHash")]
    pub block_hash: H256,
    #[serde(default)]
    #[serde(rename = "previousBlockHash")]
    pub previous_block_hash: H256,
    #[serde(default)]
    #[serde(rename = "nextBlockHash")]
    pub next_block_hash: H256,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "blockHeight")]
    pub height: Option<u64>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_null_default")]
    pub confirmations: u32,
    pub size: Option<usize>,
    pub time: u32,
    /// The version
    pub version: i32,
    #[serde(default)]
    #[serde(rename = "merkleRoot")]
    pub merkle_root: H256,
    pub nounce: u32,
    pub bits: String,
    pub difficulty: String,
    pub tx_count: u32,
    pub txs: Vec<BlockBookTransaction>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct BlockBookTickersList {
    pub ts: u64,
    pub available_currencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "test", serde(deny_unknown_fields))]
pub struct BlockBookTickers {
    pub ts: u64,
    pub rates: HashMap<String, f64>,
}
