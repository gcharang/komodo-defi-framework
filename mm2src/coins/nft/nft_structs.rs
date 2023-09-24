use crate::nft::eth_addr_to_hex;
use crate::{TransactionType, TxFeeDetails, WithdrawFee};
use common::ten;
use ethereum_types::Address;
use futures::lock::Mutex as AsyncMutex;
use mm2_core::mm_ctx::{from_ctx, MmArc};
use mm2_number::BigDecimal;
use rpc::v1::types::Bytes as BytesJson;
use serde::de::{self, Deserializer};
use serde::Deserialize;
use serde_json::Value as Json;
use std::collections::HashMap;
use std::fmt;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use url::Url;

use crate::nft::nft_errors::ParseChainTypeError;
#[cfg(target_arch = "wasm32")]
use mm2_db::indexed_db::{ConstructibleDb, SharedDb};

#[cfg(target_arch = "wasm32")]
use crate::nft::storage::wasm::nft_idb::NftCacheIDB;

/// Represents a request to list NFTs owned by the user across specified chains.
///
/// The request provides options such as pagination, limiting the number of results,
/// and applying specific filters to the list.
#[derive(Debug, Deserialize)]
pub struct NftListReq {
    /// List of chains to fetch the NFTs from.
    pub(crate) chains: Vec<Chain>,
    /// Parameter indicating if the maximum number of NFTs should be fetched.
    /// If true, then `limit` will be ignored.
    #[serde(default)]
    pub(crate) max: bool,
    /// Limit to the number of NFTs returned in a single request.
    #[serde(default = "ten")]
    pub(crate) limit: usize,
    /// Page number for pagination.
    pub(crate) page_number: Option<NonZeroUsize>,
    /// Flag indicating if the returned list should be protected from potential spam.
    #[serde(default)]
    pub(crate) protect_from_spam: bool,
    /// Optional filters to apply when listing the NFTs.
    pub(crate) filters: Option<NftListFilters>,
}

/// Filters that can be applied when listing NFTs to exclude potential threats or nuisances.
#[derive(Copy, Clone, Debug, Deserialize)]
pub struct NftListFilters {
    /// Exclude NFTs that are flagged as possible spam.
    #[serde(default)]
    pub(crate) exclude_spam: bool,
    /// Exclude NFTs that are flagged as phishing attempts.
    #[serde(default)]
    pub(crate) exclude_phishing: bool,
}

/// Contains parameters required to fetch metadata for a specified NFT.
/// # Fields
/// * `token_address`: The address of the NFT token.
/// * `token_id`: The ID of the NFT token.
/// * `chain`: The blockchain where the NFT exists.
/// * `protect_from_spam`: Indicates whether to check and redact potential spam. If set to true,
/// the internal function `protect_from_nft_spam` is utilized.
#[derive(Debug, Deserialize)]
pub struct NftMetadataReq {
    pub(crate) token_address: Address,
    pub(crate) token_id: BigDecimal,
    pub(crate) chain: Chain,
    #[serde(default)]
    pub(crate) protect_from_spam: bool,
}

/// Contains parameters required to refresh metadata for a specified NFT.
/// # Fields
/// * `token_address`: The address of the NFT token whose metadata needs to be refreshed.
/// * `token_id`: The ID of the NFT token.
/// * `chain`: The blockchain where the NFT exists.
/// * `url`: URL to fetch the metadata.
/// * `url_antispam`: URL used to validate if the fetched contract addresses are associated
/// with spam contracts or if domain fields in the fetched metadata match known phishing domains.
#[derive(Debug, Deserialize)]
pub struct RefreshMetadataReq {
    pub(crate) token_address: Address,
    pub(crate) token_id: BigDecimal,
    pub(crate) chain: Chain,
    pub(crate) url: Url,
    pub(crate) url_antispam: Url,
}

/// Represents blockchains which are supported by NFT feature.
/// Currently there are only EVM based chains.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Chain {
    Avalanche,
    Bsc,
    Eth,
    Fantom,
    Polygon,
}

pub(crate) trait ConvertChain {
    fn to_ticker(&self) -> String;
}

impl ConvertChain for Chain {
    fn to_ticker(&self) -> String {
        match self {
            Chain::Avalanche => "AVAX".to_owned(),
            Chain::Bsc => "BNB".to_owned(),
            Chain::Eth => "ETH".to_owned(),
            Chain::Fantom => "FTM".to_owned(),
            Chain::Polygon => "MATIC".to_owned(),
        }
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Chain::Avalanche => write!(f, "AVALANCHE"),
            Chain::Bsc => write!(f, "BSC"),
            Chain::Eth => write!(f, "ETH"),
            Chain::Fantom => write!(f, "FANTOM"),
            Chain::Polygon => write!(f, "POLYGON"),
        }
    }
}

impl FromStr for Chain {
    type Err = ParseChainTypeError;

    #[inline]
    fn from_str(s: &str) -> Result<Chain, ParseChainTypeError> {
        match s {
            "AVALANCHE" => Ok(Chain::Avalanche),
            "avalanche" => Ok(Chain::Avalanche),
            "BSC" => Ok(Chain::Bsc),
            "bsc" => Ok(Chain::Bsc),
            "ETH" => Ok(Chain::Eth),
            "eth" => Ok(Chain::Eth),
            "FANTOM" => Ok(Chain::Fantom),
            "fantom" => Ok(Chain::Fantom),
            "POLYGON" => Ok(Chain::Polygon),
            "polygon" => Ok(Chain::Polygon),
            _ => Err(ParseChainTypeError::UnsupportedChainType),
        }
    }
}

/// This implementation will use `FromStr` to deserialize `Chain`.
impl<'de> Deserialize<'de> for Chain {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(de::Error::custom)
    }
}

#[derive(Debug, Display)]
pub(crate) enum ParseContractTypeError {
    UnsupportedContractType,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum ContractType {
    Erc1155,
    Erc721,
}

impl FromStr for ContractType {
    type Err = ParseContractTypeError;

    #[inline]
    fn from_str(s: &str) -> Result<ContractType, ParseContractTypeError> {
        match s {
            "ERC1155" => Ok(ContractType::Erc1155),
            "ERC721" => Ok(ContractType::Erc721),
            _ => Err(ParseContractTypeError::UnsupportedContractType),
        }
    }
}

impl fmt::Display for ContractType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ContractType::Erc1155 => write!(f, "ERC1155"),
            ContractType::Erc721 => write!(f, "ERC721"),
        }
    }
}

/// `UriMeta` structure is the object which we create from `token_uri` and `metadata`.
///
/// `token_uri` and `metadata` usually contain either `image` or `image_url` with image url.
/// But most often nft creators use only `image` name for this value (from my observation),
/// less often they use both parameters with the same url.
///
/// I suspect this is because some APIs only look for one of these image url names, so nft creators try to satisfy all sides.
/// In any case, since there is no clear standard, we have to look for both options,
/// when we build `UriMeta` from `token_uri` or `metadata`.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct UriMeta {
    #[serde(rename = "image")]
    pub(crate) raw_image_url: Option<String>,
    pub(crate) image_url: Option<String>,
    pub(crate) image_domain: Option<String>,
    #[serde(rename = "name")]
    pub(crate) token_name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) attributes: Option<Json>,
    pub(crate) animation_url: Option<String>,
    pub(crate) animation_domain: Option<String>,
    pub(crate) external_url: Option<String>,
    pub(crate) external_domain: Option<String>,
    pub(crate) image_details: Option<Json>,
}

impl UriMeta {
    /// `try_to_fill_missing_fields_from` function doesnt change `raw_image_url` field.
    /// It tries to update `image_url` field instead, if it is None.
    /// As `image` is the original name of `raw_image_url` field in data from `token_uri` or `metadata`,
    /// try to find **Some()** in this field first.
    pub(crate) fn try_to_fill_missing_fields_from(&mut self, other: UriMeta) {
        if self.image_url.is_none() {
            self.image_url = other.raw_image_url.or(other.image_url);
        }
        if self.token_name.is_none() {
            self.token_name = other.token_name;
        }
        if self.description.is_none() {
            self.description = other.description;
        }
        if self.attributes.is_none() {
            self.attributes = other.attributes;
        }
        if self.animation_url.is_none() {
            self.animation_url = other.animation_url;
        }
        if self.external_url.is_none() {
            self.external_url = other.external_url;
        }
        if self.image_details.is_none() {
            self.image_details = other.image_details;
        }
    }
}

/// [`NftCommon`] structure contains common fields from [`Nft`] and [`NftFromMoralis`]
/// The `possible_spam` field indicates if any potential spam has been detected.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NftCommon {
    pub(crate) token_address: Address,
    pub(crate) token_id: BigDecimal,
    pub(crate) amount: BigDecimal,
    pub(crate) owner_of: Address,
    pub(crate) token_hash: Option<String>,
    #[serde(rename = "name")]
    pub(crate) collection_name: Option<String>,
    pub(crate) symbol: Option<String>,
    pub(crate) token_uri: Option<String>,
    pub(crate) token_domain: Option<String>,
    pub(crate) metadata: Option<String>,
    pub(crate) last_token_uri_sync: Option<String>,
    pub(crate) last_metadata_sync: Option<String>,
    pub(crate) minter_address: Option<String>,
    #[serde(default)]
    pub(crate) possible_spam: bool,
}

/// Represents an NFT with specific chain details, contract type, and other relevant attributes.
/// This structure captures detailed information about an NFT. The `possible_phishing`
/// field indicates if any domains associated with the NFT have been marked as phishing.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Nft {
    #[serde(flatten)]
    pub(crate) common: NftCommon,
    pub(crate) chain: Chain,
    pub(crate) block_number_minted: Option<u64>,
    pub(crate) block_number: u64,
    pub(crate) contract_type: ContractType,
    #[serde(default)]
    pub(crate) possible_phishing: bool,
    pub(crate) uri_meta: UriMeta,
}

pub(crate) struct BuildNftFields {
    pub(crate) token_address: Address,
    pub(crate) token_id: BigDecimal,
    pub(crate) amount: BigDecimal,
    pub(crate) owner_of: Address,
    pub(crate) contract_type: ContractType,
    pub(crate) possible_spam: bool,
    pub(crate) chain: Chain,
    pub(crate) block_number: u64,
}

pub(crate) fn build_nft_with_empty_meta(nft_fields: BuildNftFields) -> Nft {
    Nft {
        common: NftCommon {
            token_address: nft_fields.token_address,
            token_id: nft_fields.token_id,
            amount: nft_fields.amount,
            owner_of: nft_fields.owner_of,
            token_hash: None,
            collection_name: None,
            symbol: None,
            token_uri: None,
            token_domain: None,
            metadata: None,
            last_token_uri_sync: None,
            last_metadata_sync: None,
            minter_address: None,
            possible_spam: nft_fields.possible_spam,
        },
        chain: nft_fields.chain,
        block_number_minted: None,
        block_number: nft_fields.block_number,
        contract_type: nft_fields.contract_type,
        possible_phishing: false,
        uri_meta: Default::default(),
    }
}

/// Represents an NFT structure specifically for deserialization from Moralis's JSON response.
///
/// This structure is adapted to the specific format provided by Moralis's API.
#[derive(Debug, Deserialize)]
pub(crate) struct NftFromMoralis {
    #[serde(flatten)]
    pub(crate) common: NftCommon,
    pub(crate) block_number_minted: Option<SerdeStringWrap<u64>>,
    pub(crate) block_number: SerdeStringWrap<u64>,
    pub(crate) contract_type: Option<ContractType>,
}

#[derive(Debug)]
pub(crate) struct SerdeStringWrap<T>(pub(crate) T);

impl<'de, T> Deserialize<'de> for SerdeStringWrap<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug + std::fmt::Display,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: &str = Deserialize::deserialize(deserializer)?;
        let value: T = match value.parse() {
            Ok(v) => v,
            Err(e) => return Err(<D::Error as serde::de::Error>::custom(e)),
        };
        Ok(SerdeStringWrap(value))
    }
}

impl<T> std::ops::Deref for SerdeStringWrap<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}

/// Represents a detailed list of NFTs, including the total number of NFTs and the number of skipped NFTs.
/// It is used as response of `get_nft_list` if it is successful.
#[derive(Debug, Serialize)]
pub struct NftList {
    pub(crate) nfts: Vec<Nft>,
    pub(crate) skipped: usize,
    pub(crate) total: usize,
}

#[derive(Clone, Deserialize)]
pub struct WithdrawErc1155 {
    pub(crate) chain: Chain,
    pub(crate) to: String,
    pub(crate) token_address: String,
    pub(crate) token_id: BigDecimal,
    pub(crate) amount: Option<BigDecimal>,
    #[serde(default)]
    pub(crate) max: bool,
    pub(crate) fee: Option<WithdrawFee>,
}

#[derive(Clone, Deserialize)]
pub struct WithdrawErc721 {
    pub(crate) chain: Chain,
    pub(crate) to: String,
    pub(crate) token_address: String,
    pub(crate) token_id: BigDecimal,
    pub(crate) fee: Option<WithdrawFee>,
}

#[derive(Clone, Deserialize)]
#[serde(tag = "type", content = "withdraw_data")]
#[serde(rename_all = "snake_case")]
pub enum WithdrawNftReq {
    WithdrawErc1155(WithdrawErc1155),
    WithdrawErc721(WithdrawErc721),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TransactionNftDetails {
    /// Raw bytes of signed transaction, this should be sent as is to `send_raw_transaction` RPC to broadcast the transaction
    pub(crate) tx_hex: BytesJson,
    pub(crate) tx_hash: String,
    /// NFTs are sent from these addresses
    pub(crate) from: Vec<String>,
    /// NFTs are sent to these addresses
    pub(crate) to: Vec<String>,
    pub(crate) contract_type: ContractType,
    pub(crate) token_address: String,
    pub(crate) token_id: BigDecimal,
    pub(crate) amount: BigDecimal,
    pub(crate) fee_details: Option<TxFeeDetails>,
    /// The coin transaction belongs to
    pub(crate) coin: String,
    /// Block height
    pub(crate) block_height: u64,
    /// Transaction timestamp
    pub(crate) timestamp: u64,
    /// Internal MM2 id used for internal transaction identification, for some coins it might be equal to transaction hash
    pub(crate) internal_id: i64,
    /// Type of transactions, default is StandardTransfer
    #[serde(default)]
    pub(crate) transaction_type: TransactionType,
}

/// Represents a request to fetch the transfer history of NFTs owned by the user across specified chains.
///
/// The request provides options such as pagination, limiting the number of results,
/// and applying specific filters to the history.
#[derive(Debug, Deserialize)]
pub struct NftTransfersReq {
    /// List of chains to fetch the NFT transfer history from.
    pub(crate) chains: Vec<Chain>,
    /// Optional filters to apply when fetching the NFT transfer history.
    pub(crate) filters: Option<NftTransferHistoryFilters>,
    /// Parameter indicating if the maximum number of transfer records should be fetched.
    /// If true, then `limit` will be ignored.
    #[serde(default)]
    pub(crate) max: bool,
    /// Limit to the number of transfer records returned in a single request.
    #[serde(default = "ten")]
    pub(crate) limit: usize,
    /// Page number for pagination.
    pub(crate) page_number: Option<NonZeroUsize>,
    /// Flag indicating if the returned transfer history should be protected from potential spam.
    #[serde(default)]
    pub(crate) protect_from_spam: bool,
}

#[derive(Debug, Display)]
pub(crate) enum ParseTransferStatusError {
    UnsupportedTransferStatus,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) enum TransferStatus {
    Receive,
    Send,
}

impl FromStr for TransferStatus {
    type Err = ParseTransferStatusError;

    #[inline]
    fn from_str(s: &str) -> Result<TransferStatus, ParseTransferStatusError> {
        match s {
            "Receive" => Ok(TransferStatus::Receive),
            "Send" => Ok(TransferStatus::Send),
            _ => Err(ParseTransferStatusError::UnsupportedTransferStatus),
        }
    }
}

impl fmt::Display for TransferStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TransferStatus::Receive => write!(f, "Receive"),
            TransferStatus::Send => write!(f, "Send"),
        }
    }
}

/// [`NftTransferCommon`] structure contains common fields from [`NftTransferHistory`] and [`NftTransferHistoryFromMoralis`]
/// The `possible_spam` field indicates if any potential spam has been detected.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NftTransferCommon {
    pub(crate) block_hash: Option<String>,
    /// Transaction hash in hexadecimal format
    pub(crate) transaction_hash: String,
    pub(crate) transaction_index: Option<u32>,
    pub(crate) log_index: u32,
    pub(crate) value: Option<BigDecimal>,
    pub(crate) transaction_type: Option<String>,
    pub(crate) token_address: Address,
    pub(crate) token_id: BigDecimal,
    pub(crate) from_address: Address,
    pub(crate) to_address: Address,
    pub(crate) amount: BigDecimal,
    pub(crate) verified: Option<u32>,
    pub(crate) operator: Option<String>,
    #[serde(default)]
    pub(crate) possible_spam: bool,
}

/// Represents the historical transfer details of an NFT.
///
/// Contains relevant information about the NFT transfer such as the chain, block details,
/// and contract type. Additionally, fields like `collection_name`, `token_name`, and
/// urls to metadata provide insight into the NFT's identity. The `possible_phishing`
/// field indicates if any domains associated with the NFT have been marked as phishing.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NftTransferHistory {
    #[serde(flatten)]
    pub(crate) common: NftTransferCommon,
    pub(crate) chain: Chain,
    pub(crate) block_number: u64,
    pub(crate) block_timestamp: u64,
    pub(crate) contract_type: ContractType,
    pub(crate) token_uri: Option<String>,
    pub(crate) token_domain: Option<String>,
    pub(crate) collection_name: Option<String>,
    pub(crate) image_url: Option<String>,
    pub(crate) image_domain: Option<String>,
    pub(crate) token_name: Option<String>,
    pub(crate) status: TransferStatus,
    #[serde(default)]
    pub(crate) possible_phishing: bool,
}

/// Represents an NFT transfer structure specifically for deserialization from Moralis's JSON response.
///
/// This structure is adapted to the specific format provided by Moralis's API.
#[derive(Debug, Deserialize)]
pub(crate) struct NftTransferHistoryFromMoralis {
    #[serde(flatten)]
    pub(crate) common: NftTransferCommon,
    pub(crate) block_number: SerdeStringWrap<u64>,
    pub(crate) block_timestamp: String,
    pub(crate) contract_type: Option<ContractType>,
}

/// Represents the detailed transfer history of NFTs, including the total number of transfers
/// and the number of skipped transfers.
/// It is used as a response of `get_nft_transfers` if it is successful.
#[derive(Debug, Serialize)]
pub struct NftsTransferHistoryList {
    pub(crate) transfer_history: Vec<NftTransferHistory>,
    pub(crate) skipped: usize,
    pub(crate) total: usize,
}

/// Filters that can be applied to the NFT transfer history.
///
/// Allows filtering based on transaction type (send/receive), date range,
/// and whether to exclude spam or phishing-related transfers.
#[derive(Copy, Clone, Debug, Deserialize)]
pub struct NftTransferHistoryFilters {
    #[serde(default)]
    pub(crate) receive: bool,
    #[serde(default)]
    pub(crate) send: bool,
    pub(crate) from_date: Option<u64>,
    pub(crate) to_date: Option<u64>,
    #[serde(default)]
    pub(crate) exclude_spam: bool,
    #[serde(default)]
    pub(crate) exclude_phishing: bool,
}

/// Contains parameters required to update NFT transfer history and NFT list.
/// # Fields
/// * `chains`: A list of blockchains for which the NFTs need to be updated.
/// * `url`: URL to fetch the NFT data.
/// * `url_antispam`: URL used to validate if the fetched contract addresses are associated
/// with spam contracts or if domain fields in the fetched metadata match known phishing domains.
#[derive(Debug, Deserialize)]
pub struct UpdateNftReq {
    pub(crate) chains: Vec<Chain>,
    pub(crate) url: Url,
    pub(crate) url_antispam: Url,
}

#[derive(Debug, Deserialize, Eq, Hash, PartialEq)]
pub struct NftTokenAddrId {
    pub(crate) token_address: String,
    pub(crate) token_id: BigDecimal,
}

#[derive(Debug)]
pub struct TransferMeta {
    pub(crate) token_address: String,
    pub(crate) token_id: BigDecimal,
    pub(crate) token_uri: Option<String>,
    pub(crate) token_domain: Option<String>,
    pub(crate) collection_name: Option<String>,
    pub(crate) image_url: Option<String>,
    pub(crate) image_domain: Option<String>,
    pub(crate) token_name: Option<String>,
}

impl From<Nft> for TransferMeta {
    fn from(nft_db: Nft) -> Self {
        TransferMeta {
            token_address: eth_addr_to_hex(&nft_db.common.token_address),
            token_id: nft_db.common.token_id,
            token_uri: nft_db.common.token_uri,
            token_domain: nft_db.common.token_domain,
            collection_name: nft_db.common.collection_name,
            image_url: nft_db.uri_meta.image_url,
            image_domain: nft_db.uri_meta.image_domain,
            token_name: nft_db.uri_meta.token_name,
        }
    }
}

/// The primary context for NFT operations within the MM environment.
///
/// This struct provides an interface for interacting with the underlying data structures
/// required for NFT operations, including guarding against concurrent accesses and
/// dealing with platform-specific storage mechanisms.
pub(crate) struct NftCtx {
    /// An asynchronous mutex to guard against concurrent NFT operations, ensuring data consistency.
    pub(crate) guard: Arc<AsyncMutex<()>>,
    #[cfg(target_arch = "wasm32")]
    /// Platform-specific database for caching NFT data.
    pub(crate) nft_cache_db: SharedDb<NftCacheIDB>,
}

impl NftCtx {
    /// Create a new `NftCtx` from the given MM context.
    ///
    /// If an `NftCtx` instance doesn't already exist in the MM context, it gets created and cached for subsequent use.
    pub(crate) fn from_ctx(ctx: &MmArc) -> Result<Arc<NftCtx>, String> {
        Ok(try_s!(from_ctx(&ctx.nft_ctx, move || {
            Ok(NftCtx {
                guard: Arc::new(AsyncMutex::new(())),
                #[cfg(target_arch = "wasm32")]
                nft_cache_db: ConstructibleDb::new(ctx).into_shared(),
            })
        })))
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SpamContractReq {
    pub(crate) network: Chain,
    pub(crate) addresses: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct PhishingDomainReq {
    pub(crate) domains: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpamContractRes {
    pub(crate) result: HashMap<Address, bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PhishingDomainRes {
    pub(crate) result: HashMap<String, bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(crate) struct MnemonicHQRes {
    pub(crate) network: Chain,
    pub(crate) address: Address,
    pub(crate) result: String,
    pub(crate) spam_contracts: Vec<Address>,
}
