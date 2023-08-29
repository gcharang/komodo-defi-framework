use super::WalletDbShared;
use crate::z_coin::storage::{WalletDbError, ZcoinStorageError};
use crate::z_coin::{ZCoinBuilder, ZcoinConsensusParams};

use async_trait::async_trait;
use mm2_db::indexed_db::{BeBigUint, ConstructibleDb, DbIdentifier, DbInstance, DbLocked, DbUpgrader, IndexedDb,
                         IndexedDbBuilder, InitDbResult, MultiIndex, OnUpgradeResult, SharedDb, TableSignature};
use mm2_err_handle::prelude::*;
use num_traits::ToPrimitive;
use std::collections::HashMap;
use std::convert::TryFrom;
use zcash_client_backend::data_api::{PrunedBlock, ReceivedTransaction, SentTransaction};
use zcash_client_backend::encoding::{decode_extended_full_viewing_key, decode_payment_address};
use zcash_client_backend::wallet::{AccountId, SpendableNote};
use zcash_extras::{WalletRead, WalletWrite};
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::{BlockHeight, Parameters};
use zcash_primitives::memo::{Memo, MemoBytes};
use zcash_primitives::merkle_tree::{CommitmentTree, IncrementalWitness};
use zcash_primitives::sapling::{Node, Nullifier, PaymentAddress};
use zcash_primitives::transaction::components::Amount;
use zcash_primitives::transaction::TxId;
use zcash_primitives::zip32::ExtendedFullViewingKey;

const DB_NAME: &str = "wallet_db_cache";
const DB_VERSION: u32 = 1;

pub type WalletDbRes<T> = MmResult<T, ZcoinStorageError>;
pub type WalletDbInnerLocked<'a> = DbLocked<'a, WalletDbInner>;

impl<'a> WalletDbShared {
    pub async fn new(zcoin_builder: &ZCoinBuilder<'a>) -> MmResult<Self, ZcoinStorageError> {
        let ticker = zcoin_builder.ticker.clone();
        let db = WalletIndexedDb::new(zcoin_builder).await?;
        Ok(Self {
            db,
            ticker: ticker.to_string(),
        })
    }
}
pub struct WalletDbInner {
    pub inner: IndexedDb,
}

impl WalletDbInner {
    pub fn get_inner(&self) -> &IndexedDb { &self.inner }
}

#[async_trait]
impl DbInstance for WalletDbInner {
    fn db_name() -> &'static str { DB_NAME }

    async fn init(db_id: DbIdentifier) -> InitDbResult<Self> {
        let inner = IndexedDbBuilder::new(db_id)
            .with_version(DB_VERSION)
            .with_table::<WalletDbAccountsTable>()
            .with_table::<WalletDbBlocksTable>()
            .with_table::<WalletDbSaplingWitnessesTable>()
            .with_table::<WalletDbSentNotesTable>()
            .with_table::<WalletDbTransactionsTable>()
            .with_table::<WalletDbReceivedNotesTable>()
            .build()
            .await?;

        Ok(Self { inner })
    }
}

#[derive(Clone)]
pub struct WalletIndexedDb {
    pub db: SharedDb<WalletDbInner>,
    pub ticker: String,
    pub params: ZcoinConsensusParams,
}

impl<'a> WalletIndexedDb {
    pub async fn new(zcoin_builder: &ZCoinBuilder<'a>) -> MmResult<Self, ZcoinStorageError> {
        Ok(Self {
            db: ConstructibleDb::new(zcoin_builder.ctx).into_shared(),
            ticker: zcoin_builder.ticker.to_string(),
            params: zcoin_builder.protocol_info.consensus_params.clone(),
        })
    }

    #[allow(unused)]
    async fn lock_db(&self) -> WalletDbRes<WalletDbInnerLocked<'_>> {
        self.db
            .get_or_initialize()
            .await
            .mm_err(|err| ZcoinStorageError::DbError(err.to_string()))
    }
}

#[derive(Debug, Copy, Clone)]
pub enum NoteId {
    SentNoteId(i64),
    ReceivedNoteId(i64),
}

#[async_trait]
impl WalletRead for WalletIndexedDb {
    type Error = ZcoinStorageError;
    type NoteRef = NoteId;
    type TxRef = i64;

    async fn block_height_extrema(&self) -> Result<Option<(BlockHeight, BlockHeight)>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| ZcoinStorageError::add_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| ZcoinStorageError::add_err(&ticker, err.to_string()))?;
        let block_headers_db = db_transaction
            .table::<WalletDbBlocksTable>()
            .await
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
        let maybe_max_item = block_headers_db
            .cursor_builder()
            .only("ticker", ticker.clone())
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            // We need to provide any constraint on the `height` property
            // since `ticker_height` consists of both `ticker` and `height` properties.
            .bound("height", BeBigUint::from(0u64), BeBigUint::from(u64::MAX))
            // Cursor returns values from the lowest to highest key indexes.
            // But we need to get the most highest height, so reverse the cursor direction.
            .reverse()
            .open_cursor(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            .next()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
        let max = maybe_max_item.map(|(_, item)| item.height);

        let maybe_min_item = block_headers_db
            .cursor_builder()
            .only("ticker", ticker.clone())
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            // We need to provide any constraint on the `height` property
            // since `ticker_height` consists of both `ticker` and `height` properties.
            .bound("height", 0u32, u32::MAX)
            .open_cursor(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            .next()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;

        let min = maybe_min_item.map(|(_, item)| item.height);

        if let (Some(max), Some(min)) = (max, min) {
            Ok(Some((BlockHeight::from(max), BlockHeight::from(min))))
        } else {
            Ok(None)
        }
    }

    async fn get_block_hash(&self, block_height: BlockHeight) -> Result<Option<BlockHash>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| ZcoinStorageError::add_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| ZcoinStorageError::add_err(&ticker, err.to_string()))?;
        let block_headers_db = db_transaction
            .table::<WalletDbBlocksTable>()
            .await
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
        let index_keys = MultiIndex::new(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .with_value(&ticker)
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?
            .with_value(u32::from(block_height))
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;

        Ok(block_headers_db
            .get_item_by_unique_multi_index(index_keys)
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            .map(|(_, block)| BlockHash::from_slice(&block.hash.as_bytes())))
    }

    async fn get_tx_height(&self, txid: TxId) -> Result<Option<BlockHeight>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| ZcoinStorageError::add_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| ZcoinStorageError::add_err(&ticker, err.to_string()))?;
        let block_headers_db = db_transaction
            .table::<WalletDbTransactionsTable>()
            .await
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
        let index_keys = MultiIndex::new(WalletDbTransactionsTable::TICKER_TXID_INDEX)
            .with_value(&ticker)
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?
            .with_value(txid.to_string())
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;

        Ok(block_headers_db
            .get_item_by_unique_multi_index(index_keys)
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            .map(|(_, block)| BlockHeight::from(block.block)))
    }

    async fn get_address(&self, account: AccountId) -> Result<Option<PaymentAddress>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| ZcoinStorageError::add_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| ZcoinStorageError::add_err(&ticker, err.to_string()))?;
        let block_headers_db = db_transaction
            .table::<WalletDbAccountsTable>()
            .await
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
        let index_keys = MultiIndex::new(WalletDbAccountsTable::TICKER_ACCOUNT_INDEX)
            .with_value(&ticker)
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?
            .with_value(account.0)
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;

        let address = block_headers_db
            .get_item_by_unique_multi_index(index_keys)
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            .map(|(_, account)| account.address);

        if let Some(addr) = address {
            return decode_payment_address(self.params.hrp_sapling_payment_address(), &addr).map_err(|err| {
                ZcoinStorageError::DecodingError(format!(
                    "Error occurred while decoding account address: {err:?} - ticker: {ticker}"
                ))
            });
        }

        Ok(None)
    }

    async fn get_extended_full_viewing_keys(&self) -> Result<HashMap<AccountId, ExtendedFullViewingKey>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
        let accounts_table = db_transaction
            .table::<WalletDbAccountsTable>()
            .await
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
        let accounts = accounts_table
            .get_all_items()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;

        let mut res_accounts: HashMap<AccountId, ExtendedFullViewingKey> = HashMap::new();
        for (_, account) in accounts {
            let extfvk =
                decode_extended_full_viewing_key(self.params.hrp_sapling_extended_full_viewing_key(), &account.extfvk)
                    .map_err(|err| ZcoinStorageError::DecodingError(format!("{err:?} - ticker: {ticker}")))
                    .and_then(|k| k.ok_or(ZcoinStorageError::IncorrectHrpExtFvk));
            res_accounts.insert(AccountId(account.account), extfvk?);
        }

        Ok(res_accounts)
    }

    async fn is_valid_account_extfvk(
        &self,
        account: AccountId,
        extfvk: &ExtendedFullViewingKey,
    ) -> Result<bool, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
        let accounts_table = db_transaction
            .table::<WalletDbAccountsTable>()
            .await
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
        let index_keys = MultiIndex::new(WalletDbAccountsTable::TICKER_ACCOUNT_INDEX)
            .with_value(&ticker)
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?
            .with_value(account.0)
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;

        let account = accounts_table
            .get_item_by_unique_multi_index(index_keys)
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;

        if let Some((_, account)) = account {
            let expected =
                decode_extended_full_viewing_key(self.params.hrp_sapling_extended_full_viewing_key(), &account.extfvk)
                    .map_err(|err| ZcoinStorageError::DecodingError(format!("{err:?} - ticker: {ticker}")))
                    .and_then(|k| k.ok_or(ZcoinStorageError::IncorrectHrpExtFvk))?;

            return Ok(&expected == extfvk);
        }

        Ok(false)
    }

    async fn get_balance_at(&self, account: AccountId, anchor_height: BlockHeight) -> Result<Amount, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;

        let tx_table = db_transaction
            .table::<WalletDbTransactionsTable>()
            .await
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
        let mut maybe_txs = tx_table
            .cursor_builder()
            .only("ticker", ticker.clone())
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            .bound("block", 0u32, anchor_height.into())
            .open_cursor("ticker")
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;

        // Retrieves a list of transaction IDs (id_tx) from the transactions table
        // that match the provided account ID and have not been spent (spent IS NULL).
        let mut id_tx = vec![];
        while let Some((_, account)) = maybe_txs
            .next()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
        {
            id_tx.push(account.id_tx)
        }

        let received_notes_table = db_transaction
            .table::<WalletDbReceivedNotesTable>()
            .await
            .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
        let mut maybe_notes = received_notes_table
            .cursor_builder()
            .only("ticker", ticker.clone())
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            .only("account", account.0)
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
            .open_cursor(WalletDbReceivedNotesTable::TICKER_ACCOUNT_INDEX)
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;

        let mut value: i64 = 0;
        while let Some((_, note)) = maybe_notes
            .next()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?
        {
            if id_tx.contains(&note.tx) && !note.spent.is_some() {
                value += note
                    .value
                    .to_i64()
                    .ok_or_else(|| ZcoinStorageError::get_err(&ticker, "price is too large".to_string()))?
            }
        }

        match Amount::from_i64(value) {
            Ok(amount) if !amount.is_negative() => Ok(amount),
            _ => Err(ZcoinStorageError::CorruptedData(
                "Sum of values in received_notes is out of range".to_string(),
            )),
        }
    }

    async fn get_memo(&self, id_note: Self::NoteRef) -> Result<Memo, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self
            .lock_db()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
        let db_transaction = locked_db
            .get_inner()
            .transaction()
            .await
            .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;

        let memo = match id_note {
            NoteId::SentNoteId(id_note) => {
                let sent_notes_table = db_transaction
                    .table::<WalletDbSentNotesTable>()
                    .await
                    .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
                let index_keys = MultiIndex::new(WalletDbSentNotesTable::TICKER_ID_NOTE_INDEX)
                    .with_value(&ticker)
                    .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?
                    .with_value(id_note)
                    .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;

                let note = sent_notes_table
                    .get_item_by_unique_multi_index(index_keys)
                    .await
                    .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
                note.map(|(_, n)| n.memo)
            },
            NoteId::ReceivedNoteId(id_note) => {
                let received_notes_table = db_transaction
                    .table::<WalletDbSentNotesTable>()
                    .await
                    .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;
                let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_ID_NOTE_INDEX)
                    .with_value(&ticker)
                    .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?
                    .with_value(id_note)
                    .map_err(|err| ZcoinStorageError::table_err(&ticker, err.to_string()))?;

                let note = received_notes_table
                    .get_item_by_unique_multi_index(index_keys)
                    .await
                    .map_err(|err| ZcoinStorageError::get_err(&ticker, err.to_string()))?;
                note.map(|(_, n)| n.memo)
            },
        };

        if let Some(memo) = memo {
            return Ok(MemoBytes::from_bytes(&memo.as_bytes())
                .and_then(Memo::try_from)
                .map_err(|err| ZcoinStorageError::InvalidMemo(err.to_string()))?);
        };

        Err(ZcoinStorageError::get_err(&ticker, "Memo not found".to_string()))
    }

    async fn get_commitment_tree(
        &self,
        _block_height: BlockHeight,
    ) -> Result<Option<CommitmentTree<Node>>, Self::Error> {
        todo!()
    }

    async fn get_witnesses(
        &self,
        _block_height: BlockHeight,
    ) -> Result<Vec<(Self::NoteRef, IncrementalWitness<Node>)>, Self::Error> {
        todo!()
    }

    async fn get_nullifiers(&self) -> Result<Vec<(AccountId, Nullifier)>, Self::Error> { todo!() }

    async fn get_spendable_notes(
        &self,
        _account: AccountId,
        _anchor_height: BlockHeight,
    ) -> Result<Vec<SpendableNote>, Self::Error> {
        todo!()
    }

    async fn select_spendable_notes(
        &self,
        _account: AccountId,
        _target_value: Amount,
        _anchor_height: BlockHeight,
    ) -> Result<Vec<SpendableNote>, Self::Error> {
        todo!()
    }
}

#[derive(Clone)]
pub struct DataConnStmtCacheWasm {
    pub inner: SharedDb<WalletDbInner>,
}

impl DataConnStmtCacheWasm {
    pub fn transactionally<F, A>(&mut self, _f: F) -> Result<A, WalletDbError> { todo!() }
}

#[async_trait]
impl WalletRead for DataConnStmtCacheWasm {
    type Error = ();
    type NoteRef = ();
    type TxRef = ();

    async fn block_height_extrema(&self) -> Result<Option<(BlockHeight, BlockHeight)>, Self::Error> { todo!() }

    async fn get_block_hash(&self, _block_height: BlockHeight) -> Result<Option<BlockHash>, Self::Error> { todo!() }

    async fn get_tx_height(&self, _txid: TxId) -> Result<Option<BlockHeight>, Self::Error> { todo!() }

    async fn get_address(&self, _account: AccountId) -> Result<Option<PaymentAddress>, Self::Error> { todo!() }

    async fn get_extended_full_viewing_keys(&self) -> Result<HashMap<AccountId, ExtendedFullViewingKey>, Self::Error> {
        todo!()
    }

    async fn is_valid_account_extfvk(
        &self,
        _account: AccountId,
        _extfvk: &ExtendedFullViewingKey,
    ) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn get_balance_at(&self, _account: AccountId, _anchor_height: BlockHeight) -> Result<Amount, Self::Error> {
        todo!()
    }

    async fn get_memo(&self, _id_note: Self::NoteRef) -> Result<Memo, Self::Error> { todo!() }

    async fn get_commitment_tree(
        &self,
        _block_height: BlockHeight,
    ) -> Result<Option<CommitmentTree<Node>>, Self::Error> {
        todo!()
    }

    async fn get_witnesses(
        &self,
        _block_height: BlockHeight,
    ) -> Result<Vec<(Self::NoteRef, IncrementalWitness<Node>)>, Self::Error> {
        todo!()
    }

    async fn get_nullifiers(&self) -> Result<Vec<(AccountId, Nullifier)>, Self::Error> { todo!() }

    async fn get_spendable_notes(
        &self,
        _account: AccountId,
        _anchor_height: BlockHeight,
    ) -> Result<Vec<SpendableNote>, Self::Error> {
        todo!()
    }

    async fn select_spendable_notes(
        &self,
        _account: AccountId,
        _target_value: Amount,
        _anchor_height: BlockHeight,
    ) -> Result<Vec<SpendableNote>, Self::Error> {
        todo!()
    }
}

#[async_trait]
impl WalletWrite for DataConnStmtCacheWasm {
    async fn advance_by_block(
        &mut self,
        _block: &PrunedBlock,
        _updated_witnesses: &[(Self::NoteRef, IncrementalWitness<Node>)],
    ) -> Result<Vec<(Self::NoteRef, IncrementalWitness<Node>)>, Self::Error> {
        todo!()
    }

    async fn store_received_tx(&mut self, _received_tx: &ReceivedTransaction) -> Result<Self::TxRef, Self::Error> {
        todo!()
    }

    async fn store_sent_tx(&mut self, _sent_tx: &SentTransaction) -> Result<Self::TxRef, Self::Error> { todo!() }

    async fn rewind_to_height(&mut self, _block_height: BlockHeight) -> Result<(), Self::Error> { todo!() }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbAccountsTable {
    account: u32,
    extfvk: String,
    address: String,
    ticker: String,
}

impl WalletDbAccountsTable {
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * account
    pub const TICKER_ACCOUNT_INDEX: &str = "ticker_account_index";
    pub const TICKER_ACCOUNT_EXTFVK_INDEX: &str = "ticker_account_extfvk_index";
}

impl TableSignature for WalletDbAccountsTable {
    fn table_name() -> &'static str { "walletdb_accounts" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_ACCOUNT_INDEX, &["ticker", "account"], true)?;
            table.create_multi_index(Self::TICKER_ACCOUNT_INDEX, &["ticker", "account", "extfvk"], false)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbBlocksTable {
    height: u32,
    hash: String,
    time: BeBigUint,
    sapling_tree: String,
    ticker: String,
}

impl WalletDbBlocksTable {
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * height
    pub const TICKER_HEIGHT_INDEX: &str = "ticker_height_index";
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * hash
    pub const TICKER_HASH_INDEX: &str = "ticker_hash_index";
}

impl TableSignature for WalletDbBlocksTable {
    fn table_name() -> &'static str { "walletdb_blocks" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_HEIGHT_INDEX, &["ticker", "height"], true)?;
            table.create_multi_index(Self::TICKER_HASH_INDEX, &["ticker", "hash"], true)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbTransactionsTable {
    id_tx: BeBigUint,
    txid: String, // unique
    created: String,
    block: u32,
    tx_index: BeBigUint,
    expiry_height: BeBigUint,
    raw: String,
    ticker: String,
}

impl WalletDbTransactionsTable {
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * id_tx
    /// * txid
    pub const TICKER_ID_TX_INDEX: &'static str = "ticker_id_tx_index";
    pub const TICKER_TXID_INDEX: &'static str = "ticker_txid_index";
}

impl TableSignature for WalletDbTransactionsTable {
    fn table_name() -> &'static str { "walletdb_transactions" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_ID_TX_INDEX, &["ticker", "id_tx", "txid"], true)?;
            table.create_multi_index(Self::TICKER_TXID_INDEX, &["ticker", "txid"], true)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbReceivedNotesTable {
    id_note: BeBigUint,
    tx: BeBigUint,
    output_index: BeBigUint,
    account: u32,
    diversifier: String,
    value: BeBigUint,
    rcm: String,
    nf: String, // unique
    is_change: BeBigUint,
    memo: String,
    spent: Option<BeBigUint>,
    ticker: String,
}

impl WalletDbReceivedNotesTable {
    pub const TICKER_ID_NOTE_INDEX: &'static str = "ticker_id_note_index";
    pub const TICKER_ACCOUNT_INDEX: &'static str = "ticker_account_index";
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * note_id
    /// * nf
    pub const TICKER_NOTES_ID_NF_INDEX: &'static str = "ticker_note_id_nf_index";
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * tx
    /// * output_index
    pub const TICKER_NOTES_TX_OUTPUT_INDEX: &'static str = "ticker_notes_tx_output_index";
}

impl TableSignature for WalletDbReceivedNotesTable {
    fn table_name() -> &'static str { "walletdb_received_notes" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_NOTES_ID_NF_INDEX, &["ticker", "id_note", "nf"], true)?;
            table.create_multi_index(Self::TICKER_ID_NOTE_INDEX, &["ticker", "id_note"], true)?;
            table.create_multi_index(
                Self::TICKER_NOTES_TX_OUTPUT_INDEX,
                &["ticker", "tx", "output_index"],
                true,
            )?;
            table.create_multi_index(Self::TICKER_ACCOUNT_INDEX, &["ticker", "account"], false)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbSaplingWitnessesTable {
    id_witness: BeBigUint,
    note: BeBigUint,
    block: BeBigUint,
    witness: String,
    ticker: String,
}

impl WalletDbSaplingWitnessesTable {
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * note
    /// * block
    pub const TICKER_NOTE_BLOCK_INDEX: &'static str = "ticker_note_block_index";
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * id_witness
    pub const TICKER_ID_WITNESS_INDEX: &'static str = "ticker_id_witness_index";
}

impl TableSignature for WalletDbSaplingWitnessesTable {
    fn table_name() -> &'static str { "walletdb_sapling_witness" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_NOTE_BLOCK_INDEX, &["ticker", "note", "block"], true)?;
            table.create_multi_index(Self::TICKER_ID_WITNESS_INDEX, &["ticker", "id_witness"], true)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbSentNotesTable {
    id_note: i64,
    tx: BeBigUint,
    output_index: BeBigUint,
    from_account: BeBigUint,
    address: String,
    value: BeBigUint,
    memo: String,
    ticker: String,
}

impl WalletDbSentNotesTable {
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * tx
    /// * output_index
    pub const TICKER_TX_OUTPUT_INDEX: &'static str = "ticker_tx_output_index";
    pub const TICKER_ID_NOTE_INDEX: &'static str = "ticker_id_note_index";
}

impl TableSignature for WalletDbSentNotesTable {
    fn table_name() -> &'static str { "walletdb_sent_notes" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_TX_OUTPUT_INDEX, &["ticker", "tx", "output_index"], true)?;
            table.create_multi_index(Self::TICKER_ID_NOTE_INDEX, &["ticker", "id_note"], true)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}
