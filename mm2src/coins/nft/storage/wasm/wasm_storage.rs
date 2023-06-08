use crate::nft::nft_structs::{Chain, ContractType, Nft, NftList, NftTransferHistory, NftsTransferHistoryList,
                              TransferStatus, TxMeta};
use crate::nft::storage::wasm::nft_idb::{NftCacheIDB, NftCacheIDBLocked};
use crate::nft::storage::wasm::{WasmNftCacheError, WasmNftCacheResult};
use crate::nft::storage::{CreateNftStorageError, NftListStorageOps, NftTokenAddrId, NftTxHistoryFilters,
                          NftTxHistoryStorageOps, RemoveNftResult};
use crate::CoinsContext;
use async_trait::async_trait;
use mm2_core::mm_ctx::MmArc;
use mm2_db::indexed_db::{DbUpgrader, MultiIndex, OnUpgradeResult, SharedDb, TableSignature};
use mm2_err_handle::map_mm_error::MapMmError;
use mm2_err_handle::map_to_mm::MapToMmResult;
use mm2_err_handle::prelude::MmResult;
use mm2_number::BigDecimal;
use serde_json::{self as json, Value as Json};
use std::cmp::Ordering;
use std::num::NonZeroUsize;

#[derive(Clone)]
pub struct IndexedDbNftStorage {
    db: SharedDb<NftCacheIDB>,
}

impl IndexedDbNftStorage {
    pub fn new(ctx: &MmArc) -> MmResult<Self, CreateNftStorageError> {
        let coins_ctx = CoinsContext::from_ctx(ctx).map_to_mm(CreateNftStorageError::Internal)?;
        Ok(IndexedDbNftStorage {
            db: coins_ctx.nft_cache_db.clone(),
        })
    }

    async fn lock_db(&self) -> WasmNftCacheResult<NftCacheIDBLocked<'_>> {
        self.db.get_or_initialize().await.mm_err(WasmNftCacheError::from)
    }

    fn take_nft_according_to_paging_opts(
        mut nfts: Vec<Nft>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
    ) -> WasmNftCacheResult<NftList> {
        let total_count = nfts.len();
        nfts.sort_by(compare_nft_block_numbers);

        let (offset, limit) = if max {
            (0, total_count)
        } else {
            match page_number {
                Some(page) => ((page.get() - 1) * limit, limit),
                None => (0, limit),
            }
        };
        Ok(NftList {
            nfts: nfts.into_iter().skip(offset).take(limit).collect(),
            skipped: offset,
            total: total_count,
        })
    }

    #[allow(dead_code)]
    fn take_txs_according_to_filters<I>(
        txs: I,
        _filters: Option<NftTxHistoryFilters>,
    ) -> WasmNftCacheResult<Vec<NftTransferHistory>>
    where
        I: Iterator<Item = NftTxHistoryTable>,
    {
        txs.filter(|_tx| todo!()).map(tx_details_from_item).collect()
    }
}

#[async_trait]
impl NftListStorageOps for IndexedDbNftStorage {
    type Error = WasmNftCacheError;

    async fn init(&self, _chain: &Chain) -> MmResult<(), Self::Error> { Ok(()) }

    async fn is_initialized(&self, _chain: &Chain) -> MmResult<bool, Self::Error> { Ok(true) }

    async fn get_nft_list(
        &self,
        chains: Vec<Chain>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
    ) -> MmResult<NftList, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let table = db_transaction.table::<NftListTable>().await?;
        let mut nfts = Vec::new();
        for chain in chains {
            let items = table.get_items("chain", chain.to_string()).await?;
            for (_item_id, item) in items.into_iter() {
                let nft_detail = nft_details_from_item(item)?;
                nfts.push(nft_detail);
            }
        }
        Self::take_nft_according_to_paging_opts(nfts, max, limit, page_number)
    }

    async fn add_nfts_to_list<I>(&self, chain: &Chain, nfts: I, last_scanned_block: u64) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = Nft> + Send + 'static,
        I::IntoIter: Send,
    {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let nft_table = db_transaction.table::<NftListTable>().await?;
        let last_scanned_block_table = db_transaction.table::<LastScannedBlockTable>().await?;
        for nft in nfts {
            let nft_item = NftListTable::from_nft(&nft)?;
            nft_table.add_item(&nft_item).await?;
        }
        let last_scanned_block = LastScannedBlockTable {
            chain: chain.to_string(),
            last_scanned_block,
        };
        last_scanned_block_table
            .replace_item_by_unique_index("chain", chain.to_string(), &last_scanned_block)
            .await?;
        Ok(())
    }

    async fn get_nft(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Option<Nft>, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let table = db_transaction.table::<NftListTable>().await?;
        let index_keys = MultiIndex::new(NftListTable::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX)
            .with_value(chain.to_string())?
            .with_value(&token_address)?
            .with_value(token_id.to_string())?;
        if let Some((_item_id, item)) = table.get_item_by_unique_multi_index(index_keys).await? {
            Ok(Some(nft_details_from_item(item)?))
        } else {
            return Ok(None);
        }
    }

    async fn remove_nft_from_list(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
        scanned_block: u64,
    ) -> MmResult<RemoveNftResult, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let nft_table = db_transaction.table::<NftListTable>().await?;
        let last_scanned_block_table = db_transaction.table::<LastScannedBlockTable>().await?;

        let index_keys = MultiIndex::new(NftListTable::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX)
            .with_value(chain.to_string())?
            .with_value(&token_address)?
            .with_value(token_id.to_string())?;
        let last_scanned_block = LastScannedBlockTable {
            chain: chain.to_string(),
            last_scanned_block: scanned_block,
        };

        let nft_removed = nft_table.delete_item_by_unique_multi_index(index_keys).await?.is_some();
        last_scanned_block_table
            .replace_item_by_unique_index("chain", chain.to_string(), &last_scanned_block)
            .await?;
        if nft_removed {
            Ok(RemoveNftResult::NftRemoved)
        } else {
            Ok(RemoveNftResult::NftDidNotExist)
        }
    }

    async fn get_nft_amount(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Option<String>, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let table = db_transaction.table::<NftListTable>().await?;
        let index_keys = MultiIndex::new(NftListTable::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX)
            .with_value(chain.to_string())?
            .with_value(&token_address)?
            .with_value(token_id.to_string())?;
        if let Some((_item_id, item)) = table.get_item_by_unique_multi_index(index_keys).await? {
            Ok(Some(nft_details_from_item(item)?.amount.to_string()))
        } else {
            return Ok(None);
        }
    }

    async fn refresh_nft_metadata(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let table = db_transaction.table::<NftListTable>().await?;

        let index_keys = MultiIndex::new(NftListTable::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX)
            .with_value(chain.to_string())?
            .with_value(&nft.token_address)?
            .with_value(nft.token_id.to_string())?;
        let nft_item = NftListTable::from_nft(&nft)?;
        table.replace_item_by_unique_multi_index(index_keys, &nft_item).await?;
        Ok(())
    }

    async fn get_last_block_number(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let table = db_transaction.table::<NftListTable>().await?;
        let maybe_item = table
            .cursor_builder()
            .only("chain", chain.to_string())
            .map_err(|e| WasmNftCacheError::GetLastNftBlockError(e.to_string()))?
            .bound("block_number", 0u64, u64::MAX)
            .reverse()
            .open_cursor(NftListTable::CHAIN_BLOCK_NUMBER_INDEX)
            .await
            .map_err(|e| WasmNftCacheError::GetLastNftBlockError(e.to_string()))?
            .next()
            .await
            .map_err(|e| WasmNftCacheError::GetLastNftBlockError(e.to_string()))?;
        Ok(maybe_item.map(|(_, item)| item.block_number))
    }

    async fn get_last_scanned_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let table = db_transaction.table::<LastScannedBlockTable>().await?;
        let maybe_item = table
            .cursor_builder()
            .only("chain", chain.to_string())
            .map_err(|e| WasmNftCacheError::GetLastScannedBlockError(e.to_string()))?
            .bound("last_scanned_block", 0u64, u64::MAX)
            .reverse()
            .open_cursor("chain")
            .await
            .map_err(|e| WasmNftCacheError::GetLastScannedBlockError(e.to_string()))?
            .next()
            .await
            .map_err(|e| WasmNftCacheError::GetLastScannedBlockError(e.to_string()))?;
        Ok(maybe_item.map(|(_, item)| item.last_scanned_block))
    }

    async fn update_nft_amount(&self, chain: &Chain, nft: Nft, scanned_block: u64) -> MmResult<(), Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let nft_table = db_transaction.table::<NftListTable>().await?;
        let last_scanned_block_table = db_transaction.table::<LastScannedBlockTable>().await?;

        let index_keys = MultiIndex::new(NftListTable::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX)
            .with_value(chain.to_string())?
            .with_value(&nft.token_address)?
            .with_value(nft.token_id.to_string())?;
        let nft_item = NftListTable::from_nft(&nft)?;
        nft_table
            .replace_item_by_unique_multi_index(index_keys, &nft_item)
            .await?;
        let last_scanned_block = LastScannedBlockTable {
            chain: chain.to_string(),
            last_scanned_block: scanned_block,
        };
        last_scanned_block_table
            .replace_item_by_unique_index("chain", chain.to_string(), &last_scanned_block)
            .await?;
        Ok(())
    }

    async fn update_nft_amount_and_block_number(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let nft_table = db_transaction.table::<NftListTable>().await?;
        let last_scanned_block_table = db_transaction.table::<LastScannedBlockTable>().await?;

        let index_keys = MultiIndex::new(NftListTable::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX)
            .with_value(chain.to_string())?
            .with_value(&nft.token_address)?
            .with_value(nft.token_id.to_string())?;
        let nft_item = NftListTable::from_nft(&nft)?;
        nft_table
            .replace_item_by_unique_multi_index(index_keys, &nft_item)
            .await?;
        let last_scanned_block = LastScannedBlockTable {
            chain: chain.to_string(),
            last_scanned_block: nft.block_number,
        };
        last_scanned_block_table
            .replace_item_by_unique_index("chain", chain.to_string(), &last_scanned_block)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl NftTxHistoryStorageOps for IndexedDbNftStorage {
    type Error = WasmNftCacheError;

    async fn init(&self, _chain: &Chain) -> MmResult<(), Self::Error> { Ok(()) }

    async fn is_initialized(&self, _chain: &Chain) -> MmResult<bool, Self::Error> { Ok(true) }

    async fn get_tx_history(
        &self,
        _chains: Vec<Chain>,
        _max: bool,
        _limit: usize,
        _page_number: Option<NonZeroUsize>,
        _filters: Option<NftTxHistoryFilters>,
    ) -> MmResult<NftsTransferHistoryList, Self::Error> {
        todo!()
    }

    async fn add_txs_to_history<I>(&self, _chain: &Chain, _txs: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = NftTransferHistory> + Send + 'static,
        I::IntoIter: Send,
    {
        todo!()
    }

    async fn get_last_block_number(&self, _chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let _table = db_transaction.table::<NftTxHistoryTable>().await?;
        todo!()
    }

    async fn get_txs_from_block(
        &self,
        _chain: &Chain,
        _from_block: u64,
    ) -> MmResult<Vec<NftTransferHistory>, Self::Error> {
        todo!()
    }

    async fn get_txs_by_token_addr_id(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Vec<NftTransferHistory>, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let table = db_transaction.table::<NftTxHistoryTable>().await?;
        let index_keys = MultiIndex::new(NftTxHistoryTable::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX)
            .with_value(chain.to_string())?
            .with_value(&token_address)?
            .with_value(token_id.to_string())?;
        table
            .get_items_by_multi_index(index_keys)
            .await?
            .into_iter()
            .map(|(_item_id, item)| tx_details_from_item(item))
            .collect()
    }

    async fn get_tx_by_tx_hash(
        &self,
        chain: &Chain,
        transaction_hash: String,
    ) -> MmResult<Option<NftTransferHistory>, Self::Error> {
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let table = db_transaction.table::<NftTxHistoryTable>().await?;
        let index_keys = MultiIndex::new(NftTxHistoryTable::CHAIN_TX_HASH_INDEX)
            .with_value(chain.to_string())?
            .with_value(&transaction_hash)?;
        if let Some((_item_id, item)) = table.get_item_by_unique_multi_index(index_keys).await? {
            Ok(Some(tx_details_from_item(item)?))
        } else {
            return Ok(None);
        }
    }

    async fn update_tx_meta_by_hash(&self, _chain: &Chain, _tx: NftTransferHistory) -> MmResult<(), Self::Error> {
        todo!()
    }

    async fn update_txs_meta_by_token_addr_id(&self, _chain: &Chain, _tx_meta: TxMeta) -> MmResult<(), Self::Error> {
        todo!()
    }

    async fn get_txs_with_empty_meta(&self, _chain: &Chain) -> MmResult<Vec<NftTokenAddrId>, Self::Error> { todo!() }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct NftListTable {
    token_address: String,
    token_id: String,
    chain: String,
    amount: String,
    block_number: u64,
    contract_type: Option<ContractType>,
    details_json: Json,
}

impl NftListTable {
    const CHAIN_BLOCK_NUMBER_INDEX: &str = "chain_block_number_index";

    const CHAIN_TOKEN_ADD_TOKEN_ID_INDEX: &str = "chain_token_add_token_id_index";

    fn from_nft(nft: &Nft) -> WasmNftCacheResult<NftListTable> {
        let details_json = json::to_value(nft).map_to_mm(|e| WasmNftCacheError::ErrorSerializing(e.to_string()))?;
        Ok(NftListTable {
            token_address: nft.token_address.clone(),
            token_id: nft.token_id.to_string(),
            chain: nft.chain.to_string(),
            amount: nft.amount.to_string(),
            block_number: nft.block_number,
            contract_type: nft.contract_type,
            details_json,
        })
    }
}

impl TableSignature for NftListTable {
    fn table_name() -> &'static str { "nft_list_cache_table" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::CHAIN_BLOCK_NUMBER_INDEX, &["chain", "block_number"], false)?;
            table.create_multi_index(
                Self::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX,
                &["chain", "token_address", "token_id"],
                true,
            )?;
            table.create_index("chain", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct NftTxHistoryTable {
    transaction_hash: String,
    chain: String,
    block_number: u64,
    block_timestamp: u64,
    contract_type: ContractType,
    token_address: String,
    token_id: String,
    status: TransferStatus,
    amount: String,
    collection_name: Option<String>,
    image: Option<String>,
    token_name: Option<String>,
    details_json: Json,
}

impl NftTxHistoryTable {
    const CHAIN_TOKEN_ADD_TOKEN_ID_INDEX: &str = "chain_token_add_token_id_index";

    const CHAIN_TX_HASH_INDEX: &str = "chain_tx_hash_index";

    #[allow(dead_code)]
    fn from_tx_history(tx: &NftTransferHistory) -> WasmNftCacheResult<NftTxHistoryTable> {
        let details_json = json::to_value(tx).map_to_mm(|e| WasmNftCacheError::ErrorSerializing(e.to_string()))?;
        Ok(NftTxHistoryTable {
            transaction_hash: tx.transaction_hash.clone(),
            chain: tx.chain.to_string(),
            block_number: tx.block_number,
            block_timestamp: tx.block_timestamp,
            contract_type: tx.contract_type,
            token_address: tx.token_address.clone(),
            token_id: tx.token_id.to_string(),
            status: tx.status,
            amount: tx.amount.to_string(),
            collection_name: tx.collection_name.clone(),
            image: tx.image.clone(),
            token_name: tx.token_name.clone(),
            details_json,
        })
    }
}

impl TableSignature for NftTxHistoryTable {
    fn table_name() -> &'static str { "nft_tx_history_cache_table" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(
                Self::CHAIN_TOKEN_ADD_TOKEN_ID_INDEX,
                &["chain", "token_address", "token_id"],
                false,
            )?;
            table.create_multi_index(Self::CHAIN_TX_HASH_INDEX, &["chain", "transaction_hash"], true)?;
            table.create_index("chain", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct LastScannedBlockTable {
    chain: String,
    last_scanned_block: u64,
}

impl TableSignature for LastScannedBlockTable {
    fn table_name() -> &'static str { "last_scanned_block_table" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_index("chain", true)?;
        }
        Ok(())
    }
}

fn nft_details_from_item(item: NftListTable) -> WasmNftCacheResult<Nft> {
    json::from_value(item.details_json).map_to_mm(|e| WasmNftCacheError::ErrorDeserializing(e.to_string()))
}

fn tx_details_from_item(item: NftTxHistoryTable) -> WasmNftCacheResult<NftTransferHistory> {
    json::from_value(item.details_json).map_to_mm(|e| WasmNftCacheError::ErrorDeserializing(e.to_string()))
}

fn compare_nft_block_numbers(a: &Nft, b: &Nft) -> Ordering {
    let a = BlockNumber::new(a.block_number);
    let b = BlockNumber::new(b.block_number);
    compare_nfts(a, b)
}

struct BlockNumber {
    block_number: u64,
}

impl BlockNumber {
    fn new(block_number: u64) -> BlockNumber { BlockNumber { block_number } }
}

fn compare_nfts(a: BlockNumber, b: BlockNumber) -> Ordering {
    if a.block_number == 0 {
        Ordering::Less
    } else if b.block_number == 0 {
        Ordering::Greater
    } else {
        b.block_number.cmp(&a.block_number)
    }
}
