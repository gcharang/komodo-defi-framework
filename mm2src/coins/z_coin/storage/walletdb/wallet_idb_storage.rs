use super::WalletDbShared;
use crate::z_coin::storage::ZcoinStorageError;
use crate::z_coin::{ZCoinBuilder, ZcoinConsensusParams};

use async_trait::async_trait;
use ff::PrimeField;
use mm2_db::indexed_db::{BeBigUint, ConstructibleDb, DbIdentifier, DbInstance, DbLocked, DbUpgrader, IndexedDb,
                         IndexedDbBuilder, InitDbResult, MultiIndex, OnUpgradeResult, SharedDb, TableSignature};
use mm2_err_handle::prelude::*;
//use mm2_number::num_bigint::ToBigInt;
use mm2_number::BigInt;
use num_traits::ToPrimitive;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use zcash_client_backend::data_api::{PrunedBlock, ReceivedTransaction, SentTransaction};
use zcash_client_backend::encoding::{decode_extended_full_viewing_key, decode_payment_address};
use zcash_client_backend::wallet::{AccountId, SpendableNote};
use zcash_extras::{WalletRead, WalletWrite};
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::{BlockHeight, Parameters};
use zcash_primitives::memo::{Memo, MemoBytes};
use zcash_primitives::merkle_tree::{CommitmentTree, IncrementalWitness};
use zcash_primitives::sapling::{Diversifier, Node, Nullifier, PaymentAddress, Rseed};
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

struct SpendableNoteConstructor {
    diversifier: String,
    value: BigInt,
    rcm: String,
    witness: String,
}

fn to_spendable_note(note: SpendableNoteConstructor) -> MmResult<SpendableNote, ZcoinStorageError> {
    let diversifier = {
        let d = note.diversifier.as_bytes();
        if d.len() != 11 {
            return MmError::err(ZcoinStorageError::CorruptedData(
                "Invalid diversifier length".to_string(),
            ));
        }
        let mut tmp = [0; 11];
        tmp.copy_from_slice(&d);
        Diversifier(tmp)
    };

    let note_value = Amount::from_i64(note.value.to_i64().expect("BigInt is too large to fit in an i64")).unwrap();

    let rseed = {
        let rcm_bytes = note.rcm.as_bytes();

        // We store rcm directly in the data DB, regardless of whether the note
        // used a v1 or v2 note plaintext, so for the purposes of spending let's
        // pretend this is a pre-ZIP 212 note.
        let rcm = jubjub::Fr::from_repr(
            rcm_bytes[..]
                .try_into()
                .map_to_mm(|_| ZcoinStorageError::InvalidNote("Invalid note".to_string()))?,
        )
        .ok_or(MmError::new(ZcoinStorageError::InvalidNote("Invalid note".to_string())))?;
        Rseed::BeforeZip212(rcm)
    };

    let witness = {
        let d = note.witness.as_bytes();
        IncrementalWitness::read(&d[..]).map_to_mm(|err| ZcoinStorageError::IoError(err.to_string()))?
    };

    Ok(SpendableNote {
        diversifier,
        note_value,
        rseed,
        witness,
    })
}

#[async_trait]
impl WalletRead for WalletIndexedDb {
    type Error = MmError<ZcoinStorageError>;
    type NoteRef = NoteId;
    type TxRef = i64;

    async fn block_height_extrema(&self) -> Result<Option<(BlockHeight, BlockHeight)>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let block_headers_db = db_transaction.table::<WalletDbBlocksTable>().await?;
        let maybe_max_item = block_headers_db
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .bound("height", 0u32, u32::MAX)
            .reverse()
            .open_cursor(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .await?
            .next()
            .await?;
        let max = maybe_max_item.map(|(_, item)| item.height);

        let maybe_min_item = block_headers_db
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .bound("height", 0u32, u32::MAX)
            .open_cursor(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .await?
            .next()
            .await?;

        let min = maybe_min_item.map(|(_, item)| item.height);

        if let (Some(min), Some(max)) = (min, max) {
            Ok(Some((BlockHeight::from(min), BlockHeight::from(max))))
        } else {
            Ok(None)
        }
    }

    async fn get_block_hash(&self, block_height: BlockHeight) -> Result<Option<BlockHash>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let block_headers_db = db_transaction.table::<WalletDbBlocksTable>().await?;
        let index_keys = MultiIndex::new(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .with_value(&ticker)?
            .with_value(u32::from(block_height))?;

        Ok(block_headers_db
            .get_item_by_unique_multi_index(index_keys)
            .await?
            .map(|(_, block)| BlockHash::from_slice(&block.hash.as_bytes())))
    }

    async fn get_tx_height(&self, txid: TxId) -> Result<Option<BlockHeight>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let block_headers_db = db_transaction.table::<WalletDbTransactionsTable>().await?;
        let index_keys = MultiIndex::new(WalletDbTransactionsTable::TICKER_TXID_INDEX)
            .with_value(&ticker)?
            .with_value(txid.to_string())?;

        Ok(block_headers_db
            .get_item_by_unique_multi_index(index_keys)
            .await?
            .map(|(_, tx)| tx.block.map(|block| BlockHeight::from(block)))
            .flatten())
    }

    async fn get_address(&self, account: AccountId) -> Result<Option<PaymentAddress>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let block_headers_db = db_transaction.table::<WalletDbAccountsTable>().await?;
        let index_keys = MultiIndex::new(WalletDbAccountsTable::TICKER_ACCOUNT_INDEX)
            .with_value(&ticker)?
            .with_value(account.0)?;

        let address = block_headers_db
            .get_item_by_unique_multi_index(index_keys)
            .await?
            .map(|(_, account)| account.address);

        if let Some(addr) = address {
            return decode_payment_address(self.params.hrp_sapling_payment_address(), &addr).map_to_mm(|err| {
                ZcoinStorageError::DecodingError(format!(
                    "Error occurred while decoding account address: {err:?} - ticker: {ticker}"
                ))
            });
        }

        Ok(None)
    }

    async fn get_extended_full_viewing_keys(&self) -> Result<HashMap<AccountId, ExtendedFullViewingKey>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let accounts_table = db_transaction.table::<WalletDbAccountsTable>().await?;
        let maybe_accounts = accounts_table.get_items("ticker", &ticker).await?;

        let mut res_accounts: HashMap<AccountId, ExtendedFullViewingKey> = HashMap::new();
        for (_, account) in maybe_accounts {
            let extfvk =
                decode_extended_full_viewing_key(self.params.hrp_sapling_extended_full_viewing_key(), &account.extfvk)
                    .map_to_mm(|err| ZcoinStorageError::DecodingError(format!("{err:?} - ticker: {ticker}")))
                    .and_then(|k| k.ok_or(MmError::new(ZcoinStorageError::IncorrectHrpExtFvk)));
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
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let accounts_table = db_transaction.table::<WalletDbAccountsTable>().await?;
        let index_keys = MultiIndex::new(WalletDbAccountsTable::TICKER_ACCOUNT_INDEX)
            .with_value(&ticker)?
            .with_value(account.0)?;

        let account = accounts_table.get_item_by_unique_multi_index(index_keys).await?;

        if let Some((_, account)) = account {
            let expected =
                decode_extended_full_viewing_key(self.params.hrp_sapling_extended_full_viewing_key(), &account.extfvk)
                    .map_to_mm(|err| ZcoinStorageError::DecodingError(format!("{err:?} - ticker: {ticker}")))
                    .and_then(|k| k.ok_or(MmError::new(ZcoinStorageError::IncorrectHrpExtFvk)))?;

            return Ok(&expected == extfvk);
        }

        Ok(false)
    }

    async fn get_balance_at(&self, account: AccountId, anchor_height: BlockHeight) -> Result<Amount, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;

        let tx_table = db_transaction.table::<WalletDbTransactionsTable>().await?;
        let mut maybe_txs = tx_table
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .bound("block", 0u32, anchor_height.into())
            .open_cursor("ticker")
            .await?;

        // Retrieves a list of transaction IDs (id_tx) from the transactions table
        // that match the provided account ID and have not been spent (spent IS NULL).
        let mut id_tx = vec![];
        while let Some((_, account)) = maybe_txs.next().await? {
            id_tx.push(account.id_tx)
        }

        let received_notes_table = db_transaction.table::<WalletDbReceivedNotesTable>().await?;
        let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_ACCOUNT_INDEX)
            .with_value(&ticker)?
            .with_value(account.0)?;
        let maybe_notes = received_notes_table.get_items_by_multi_index(index_keys).await?;

        let mut value: i64 = 0;
        for (_, note) in maybe_notes {
            if id_tx.contains(&note.tx) && note.spent.is_none() {
                value += note.value.to_i64().ok_or_else(|| {
                    MmError::new(ZcoinStorageError::GetFromStorageError("price is too large".to_string()))
                })?
            }
        }

        match Amount::from_i64(value) {
            Ok(amount) if !amount.is_negative() => Ok(amount),
            _ => MmError::err(ZcoinStorageError::CorruptedData(
                "Sum of values in received_notes is out of range".to_string(),
            )),
        }
    }

    async fn get_memo(&self, id_note: Self::NoteRef) -> Result<Memo, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;

        let memo = match id_note {
            NoteId::SentNoteId(id_note) => {
                let sent_notes_table = db_transaction.table::<WalletDbSentNotesTable>().await?;
                let index_keys = MultiIndex::new(WalletDbSentNotesTable::TICKER_ID_NOTE_INDEX)
                    .with_value(&ticker)?
                    .with_value(id_note)?;

                let note = sent_notes_table.get_item_by_unique_multi_index(index_keys).await?;
                note.map(|(_, n)| n.memo)
            },
            NoteId::ReceivedNoteId(id_note) => {
                let received_notes_table = db_transaction.table::<WalletDbSentNotesTable>().await?;
                let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_ID_NOTE_INDEX)
                    .with_value(&ticker)?
                    .with_value(id_note)?;

                let note = received_notes_table.get_item_by_unique_multi_index(index_keys).await?;
                note.map(|(_, n)| n.memo)
            },
        };

        if let Some(Some(memo)) = memo {
            return Ok(MemoBytes::from_bytes(&memo.as_bytes())
                .and_then(Memo::try_from)
                .map_to_mm(|err| ZcoinStorageError::InvalidMemo(err.to_string()))?);
        };

        MmError::err(ZcoinStorageError::GetFromStorageError(format!("Memo not found")))
    }

    async fn get_commitment_tree(
        &self,
        block_height: BlockHeight,
    ) -> Result<Option<CommitmentTree<Node>>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let blocks_table = db_transaction.table::<WalletDbBlocksTable>().await?;
        let index_keys = MultiIndex::new(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .with_value(&ticker)?
            .with_value(u32::from(block_height))?;

        let block = blocks_table
            .get_item_by_unique_multi_index(index_keys)
            .await?
            .map(|(_, account)| account);

        if let Some(block) = block {
            let sapling_tree = block.sapling_tree.as_bytes();
            return Ok(Some(
                CommitmentTree::read(&sapling_tree[..])
                    .map_to_mm(|e| ZcoinStorageError::DecodingError(e.to_string()))?,
            ));
        }

        Ok(None)
    }

    async fn get_witnesses(
        &self,
        block_height: BlockHeight,
    ) -> Result<Vec<(Self::NoteRef, IncrementalWitness<Node>)>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;

        let sapling_witness_table = db_transaction.table::<WalletDbSaplingWitnessesTable>().await?;
        let mut maybe_sapling_witnesses = sapling_witness_table
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .only("block", u32::from(block_height))?
            .open_cursor("ticker")
            .await?;

        // Retrieves a list of transaction IDs (id_tx) from the transactions table
        // that match the provided account ID and have not been spent (spent IS NULL).
        let mut witnesses = vec![];
        while let Some((_, block)) = maybe_sapling_witnesses.next().await? {
            let id_note = NoteId::ReceivedNoteId(block.note.to_i64().expect("BigInt is too large to fit in an i64"));
            let witness = IncrementalWitness::read(&block.witness.as_bytes()[..])
                .map(|witness| (id_note, witness))
                .map_to_mm(|err| ZcoinStorageError::DecodingError(err.to_string()))?;
            witnesses.push(witness)
        }

        Ok(witnesses)
    }

    async fn get_nullifiers(&self) -> Result<Vec<(AccountId, Nullifier)>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;

        // Received notes
        let received_notes_table = db_transaction.table::<WalletDbReceivedNotesTable>().await?;
        let maybe_notes = received_notes_table.get_items("ticker", ticker.clone()).await?;

        // Transactions
        let txs_table = db_transaction.table::<WalletDbTransactionsTable>().await?;
        let maybe_txs = txs_table.get_items("ticker", &ticker).await?;

        let mut nullifiers = vec![];
        for (_, note) in maybe_notes {
            for (_, tx) in &maybe_txs {
                if let Some(spent) = note.spent {
                    if tx.id_tx == spent && tx.block.is_none() {
                        nullifiers.push((
                            AccountId(note.account),
                            Nullifier::from_slice(note.nf.clone().as_bytes()).unwrap(),
                        ));
                    }
                }
            }
        }

        Ok(nullifiers)
    }

    async fn get_spendable_notes(
        &self,
        account: AccountId,
        anchor_height: BlockHeight,
    ) -> Result<Vec<SpendableNote>, Self::Error> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;

        // Received notes
        let received_notes_table = db_transaction.table::<WalletDbReceivedNotesTable>().await?;
        let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_ACCOUNT_INDEX)
            .with_value(&ticker)?
            .with_value(account.0)?;
        let maybe_notes = received_notes_table.get_items_by_multi_index(index_keys).await?;
        let maybe_notes = maybe_notes.iter().filter(|(_, note)| note.spent.is_none());

        // Transactions
        let txs_table = db_transaction.table::<WalletDbTransactionsTable>().await?;
        let mut maybe_txs = txs_table
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .bound("block", 0u32, u32::from(anchor_height))
            .open_cursor(WalletDbTransactionsTable::TICKER_BLOCK_INDEX)
            .await?;
        let mut txs = vec![];
        while let Some((_, ts)) = maybe_txs.next().await? {
            txs.push(ts)
        }

        // Witnesses
        let witnesses_table = db_transaction.table::<WalletDbSaplingWitnessesTable>().await?;
        let mut maybe_witnesses = witnesses_table
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .bound("block", 0u32, u32::from(anchor_height))
            .open_cursor(WalletDbSaplingWitnessesTable::TICKER_BLOCK_INDEX)
            .await?;
        let mut witnesses = vec![];
        while let Some((_, witness)) = maybe_witnesses.next().await? {
            witnesses.push(witness)
        }

        let mut spendable_notes = vec![];
        for (_, note) in maybe_notes {
            let witness = witnesses.iter().find(|wit| wit.note == note.id_note.into());
            let tx = txs.iter().find(|tx| tx.id_tx == note.tx);

            if let (Some(witness), Some(_)) = (witness, tx) {
                let spend = SpendableNoteConstructor {
                    diversifier: note.diversifier.to_owned(),
                    value: note.value.clone(),
                    rcm: note.rcm.to_owned(),
                    witness: witness.witness.to_string(),
                };
                spendable_notes.push(to_spendable_note(spend)?);
            }
        }

        Ok(spendable_notes)
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
    pub inner: WalletIndexedDb,
}

#[async_trait]
impl WalletRead for DataConnStmtCacheWasm {
    type Error = MmError<ZcoinStorageError>;
    type NoteRef = NoteId;
    type TxRef = i64;

    async fn block_height_extrema(&self) -> Result<Option<(BlockHeight, BlockHeight)>, Self::Error> {
        self.inner.block_height_extrema().await
    }

    async fn get_block_hash(&self, block_height: BlockHeight) -> Result<Option<BlockHash>, Self::Error> {
        self.inner.get_block_hash(block_height).await
    }

    async fn get_tx_height(&self, txid: TxId) -> Result<Option<BlockHeight>, Self::Error> {
        self.inner.get_tx_height(txid).await
    }

    async fn get_address(&self, account: AccountId) -> Result<Option<PaymentAddress>, Self::Error> {
        self.inner.get_address(account).await
    }

    async fn get_extended_full_viewing_keys(&self) -> Result<HashMap<AccountId, ExtendedFullViewingKey>, Self::Error> {
        self.inner.get_extended_full_viewing_keys().await
    }

    async fn is_valid_account_extfvk(
        &self,
        account: AccountId,
        extfvk: &ExtendedFullViewingKey,
    ) -> Result<bool, Self::Error> {
        self.inner.is_valid_account_extfvk(account, extfvk).await
    }

    async fn get_balance_at(&self, account: AccountId, anchor_height: BlockHeight) -> Result<Amount, Self::Error> {
        self.inner.get_balance_at(account, anchor_height).await
    }

    async fn get_memo(&self, id_note: Self::NoteRef) -> Result<Memo, Self::Error> { self.inner.get_memo(id_note).await }

    async fn get_commitment_tree(
        &self,
        block_height: BlockHeight,
    ) -> Result<Option<CommitmentTree<Node>>, Self::Error> {
        self.inner.get_commitment_tree(block_height).await
    }

    async fn get_witnesses(
        &self,
        block_height: BlockHeight,
    ) -> Result<Vec<(Self::NoteRef, IncrementalWitness<Node>)>, Self::Error> {
        self.inner.get_witnesses(block_height).await
    }

    async fn get_nullifiers(&self) -> Result<Vec<(AccountId, Nullifier)>, Self::Error> {
        self.inner.get_nullifiers().await
    }

    async fn get_spendable_notes(
        &self,
        account: AccountId,
        anchor_height: BlockHeight,
    ) -> Result<Vec<SpendableNote>, Self::Error> {
        self.inner.get_spendable_notes(account, anchor_height).await
    }

    async fn select_spendable_notes(
        &self,
        account: AccountId,
        target_value: Amount,
        anchor_height: BlockHeight,
    ) -> Result<Vec<SpendableNote>, Self::Error> {
        self.inner
            .select_spendable_notes(account, target_value, anchor_height)
            .await
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
    id_tx: u32,
    txid: String, // unique
    created: String,
    block: Option<u32>,
    tx_index: Option<u32>,
    expiry_height: Option<u32>,
    raw: Option<String>,
    ticker: String,
}

impl WalletDbTransactionsTable {
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * id_tx
    /// * txid
    pub const TICKER_ID_TX_INDEX: &'static str = "ticker_id_tx_index";
    pub const TICKER_TXID_INDEX: &'static str = "ticker_txid_index";
    pub const TICKER_BLOCK_INDEX: &'static str = "ticker_block_index";
}

impl TableSignature for WalletDbTransactionsTable {
    fn table_name() -> &'static str { "walletdb_transactions" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_ID_TX_INDEX, &["ticker", "id_tx", "txid"], true)?;
            table.create_multi_index(Self::TICKER_TXID_INDEX, &["ticker", "txid"], true)?;
            table.create_multi_index(Self::TICKER_BLOCK_INDEX, &["ticker", "block"], false)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbReceivedNotesTable {
    id_note: u32,
    // references transactions(id_tx)
    tx: u32,
    output_index: u32,
    // references accounts(account)
    account: u32,
    diversifier: String,
    value: BigInt,
    rcm: String,
    nf: String, // unique
    is_change: BigInt,
    memo: String,
    // references transactions(id_tx)
    spent: Option<u32>,
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
    id_witness: u32,
    // REFERENCES received_notes(id_note)
    note: BigInt,
    // REFERENCES blocks(height)
    block: u32,
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
    pub const TICKER_BLOCK_INDEX: &'static str = "ticker_block_index";
}

impl TableSignature for WalletDbSaplingWitnessesTable {
    fn table_name() -> &'static str { "walletdb_sapling_witness" }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_multi_index(Self::TICKER_NOTE_BLOCK_INDEX, &["ticker", "note", "block"], true)?;
            table.create_multi_index(Self::TICKER_ID_WITNESS_INDEX, &["ticker", "id_witness"], true)?;
            table.create_multi_index(Self::TICKER_BLOCK_INDEX, &["ticker", "block"], false)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbSentNotesTable {
    id_note: BigInt,
    // REFERENCES transactions(id_tx)
    tx: BeBigUint,
    output_index: BeBigUint,
    // REFERENCES accounts(account)
    from_account: BigInt,
    address: String,
    value: BigInt,
    memo: Option<String>,
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
