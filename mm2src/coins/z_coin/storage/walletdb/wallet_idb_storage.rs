use super::WalletDbShared;
use crate::z_coin::z_coin_errors::ZcoinStorageError;
use crate::z_coin::{ZCoinBuilder, ZcoinConsensusParams};

use async_trait::async_trait;
use ff::PrimeField;
use mm2_db::indexed_db::{BeBigUint, ConstructibleDb, DbIdentifier, DbInstance, DbLocked, DbUpgrader, IndexedDb,
                         IndexedDbBuilder, InitDbResult, MultiIndex, OnUpgradeResult, SharedDb, TableSignature};
use mm2_err_handle::prelude::*;
use mm2_number::num_bigint::ToBigInt;
use mm2_number::BigInt;
use num_traits::{FromPrimitive, ToPrimitive};
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::ops::Deref;
use zcash_client_backend::data_api::{PrunedBlock, ReceivedTransaction, SentTransaction};
use zcash_client_backend::encoding::{decode_extended_full_viewing_key, decode_payment_address};
use zcash_client_backend::wallet::{AccountId, SpendableNote, WalletTx};
use zcash_extras::{ShieldedOutput, WalletRead, WalletWrite};
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::{BlockHeight, Parameters};
use zcash_primitives::memo::{Memo, MemoBytes};
use zcash_primitives::merkle_tree::{CommitmentTree, IncrementalWitness};
use zcash_primitives::sapling::{Diversifier, Node, Nullifier, PaymentAddress, Rseed};
use zcash_primitives::transaction::components::Amount;
use zcash_primitives::transaction::{Transaction, TxId};
use zcash_primitives::zip32::{ExtendedFullViewingKey, ExtendedSpendingKey};

const DB_NAME: &str = "wallet_db_cache";
const DB_VERSION: u32 = 1;

pub type WalletDbRes<T> = MmResult<T, ZcoinStorageError>;
pub type WalletDbInnerLocked<'a> = DbLocked<'a, WalletDbInner>;

impl<'a> WalletDbShared {
    pub async fn new(
        zcoin_builder: &ZCoinBuilder<'a>,
        z_spending_key: &ExtendedSpendingKey,
    ) -> MmResult<Self, ZcoinStorageError> {
        let ticker = zcoin_builder.ticker;
        let db = WalletIndexedDb::new(zcoin_builder, z_spending_key).await?;
        Ok(Self {
            db,
            ticker: ticker.to_string(),
        })
    }

    pub async fn is_tx_imported(&self, _tx_id: TxId) -> bool { todo!() }
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
    pub async fn new(
        zcoin_builder: &ZCoinBuilder<'a>,
        _z_spending_key: &ExtendedSpendingKey,
    ) -> MmResult<Self, ZcoinStorageError> {
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

impl WalletIndexedDb {
    pub async fn insert_block(
        &self,
        block_height: BlockHeight,
        block_hash: BlockHash,
        block_time: u32,
        commitment_tree: &CommitmentTree<Node>,
    ) -> MmResult<(), ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let walletdb_blocks_table = db_transaction.table::<WalletDbBlocksTable>().await?;

        let mut encoded_tree = Vec::new();
        commitment_tree.write(&mut encoded_tree).unwrap();
        let hash = &block_hash.0[..];
        let block = WalletDbBlocksTable {
            height: u32::from(block_height),
            hash: hash.to_vec(),
            time: block_time,
            sapling_tree: encoded_tree,
            ticker: ticker.clone(),
        };

        let index_keys = MultiIndex::new(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .with_value(&ticker)?
            .with_value(u32::from(block_height))?;
        walletdb_blocks_table
            .replace_item_by_unique_multi_index(index_keys, &block)
            .await?;

        Ok(())
    }

    pub async fn put_tx_data(&self, tx: &Transaction, created_at: Option<String>) -> MmResult<i64, ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let tx_table = db_transaction.table::<WalletDbTransactionsTable>().await?;

        let mut raw_tx = vec![];
        tx.write(&mut raw_tx).unwrap();
        let txid = tx.txid().0.to_vec();

        if let Some((id_tx, some_tx)) = self.get_single_tx(txid.clone()).await? {
            let updated_tx = WalletDbTransactionsTable {
                txid: txid.clone(),
                created: some_tx.created,
                block: some_tx.block,
                tx_index: some_tx.tx_index,
                expiry_height: Some(u32::from(tx.expiry_height)),
                raw: Some(raw_tx),
                ticker: ticker.clone(),
            };
            tx_table.replace_item(id_tx, &updated_tx).await?;

            return Ok(id_tx as i64);
        };

        let new_tx = WalletDbTransactionsTable {
            txid: txid.clone(),
            created: created_at,
            block: None,
            tx_index: None,
            expiry_height: Some(u32::from(tx.expiry_height)),
            raw: Some(raw_tx),
            ticker: ticker.clone(),
        };
        let index_keys = MultiIndex::new(WalletDbTransactionsTable::TICKER_TXID_INDEX)
            .with_value(&ticker)?
            .with_value(txid)?;
        let id = tx_table.replace_item_by_unique_multi_index(index_keys, &new_tx).await?;

        Ok(id.into())
    }

    pub async fn put_tx_meta<N>(&self, tx: &WalletTx<N>, height: BlockHeight) -> MmResult<i64, ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let tx_table = db_transaction.table::<WalletDbTransactionsTable>().await?;

        let txid = tx.txid.0.to_vec();

        if let Some((id_tx, some_tx)) = self.get_single_tx(txid.clone()).await? {
            let updated_tx = WalletDbTransactionsTable {
                txid: some_tx.txid.clone(),
                created: some_tx.created,
                block: Some(u32::from(height)),
                tx_index: Some(tx.index as i64),
                expiry_height: some_tx.expiry_height,
                raw: some_tx.raw,
                ticker: ticker.clone(),
            };
            tx_table.replace_item(id_tx, &updated_tx).await?;

            return Ok(id_tx as i64);
        };

        let new_tx = WalletDbTransactionsTable {
            txid: txid.clone(),
            created: None,
            block: Some(u32::from(height)),
            tx_index: Some(tx.index as i64),
            expiry_height: None,
            raw: None,
            ticker: ticker.clone(),
        };
        let index_keys = MultiIndex::new(WalletDbTransactionsTable::TICKER_TXID_INDEX)
            .with_value(&ticker)?
            .with_value(txid)?;
        let id = tx_table.replace_item_by_unique_multi_index(index_keys, &new_tx).await?;

        Ok(id.into())
    }

    pub async fn mark_spent(&self, tx_ref: i64, nf: &Nullifier) -> MmResult<(), ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let received_notes_table = db_transaction.table::<WalletDbReceivedNotesTable>().await?;

        let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_NF_INDEX)
            .with_value(&ticker)?
            .with_value(nf.0.to_vec())?;
        let maybe_note = received_notes_table.get_item_by_unique_multi_index(index_keys).await?;

        if let Some((_, note)) = maybe_note {
            let new_received_note = WalletDbReceivedNotesTable {
                tx: note.tx,
                output_index: note.output_index,
                account: note.account,
                diversifier: note.diversifier,
                value: note.value,
                rcm: note.rcm,
                nf: Some(nf.0.to_vec()),
                is_change: note.is_change,
                memo: note.memo,
                spent: Some(tx_ref.to_bigint().unwrap()),
                ticker: ticker.clone(),
            };

            let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_NF_INDEX)
                .with_value(&ticker)?
                .with_value(nf.0.to_vec())?;
            received_notes_table
                .replace_item_by_unique_multi_index(index_keys, &new_received_note)
                .await?;
        }

        MmError::err(ZcoinStorageError::GetFromStorageError("note not found".to_string()))
    }

    pub async fn put_received_note<T: ShieldedOutput>(
        &self,
        output: &T,
        tx_ref: i64,
    ) -> MmResult<NoteId, ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;

        let rcm = output.note().rcm().to_repr();
        let account = BigInt::from(output.account().0);
        let diversifier = output.to().diversifier().0.to_vec();
        let value = output.note().value.into();
        let rcm = rcm.to_vec();
        let memo = output.memo().map(|m| m.as_slice().to_vec());
        let is_change = output.is_change();
        let tx = tx_ref as u32;
        let output_index = output.index() as u32;
        let nf_bytes = output.nullifier().map(|nf| nf.0.to_vec());

        let received_note_table = db_transaction.table::<WalletDbReceivedNotesTable>().await?;
        let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_TX_OUTPUT_INDEX)
            .with_value(&ticker)?
            .with_value(tx)?
            .with_value(output_index)?;
        let current_note = received_note_table.get_item_by_unique_multi_index(index_keys).await?;

        let id = if let Some((_id, note)) = current_note {
            let temp_note = WalletDbReceivedNotesTable {
                tx,
                output_index,
                account: note.account,
                diversifier,
                value,
                rcm,
                nf: note.nf.or(nf_bytes),
                is_change: note.is_change.or(is_change),
                memo: note.memo.or(memo),
                spent: note.spent,
                ticker: ticker.clone(),
            };

            let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_TX_OUTPUT_INDEX)
                .with_value(&ticker)?
                .with_value(tx)?
                .with_value(output_index)?;
            received_note_table
                .replace_item_by_unique_multi_index(index_keys, &temp_note)
                .await?
        } else {
            let new_note = WalletDbReceivedNotesTable {
                tx,
                output_index,
                account,
                diversifier,
                value,
                rcm,
                nf: nf_bytes,
                is_change,
                memo,
                spent: None,
                ticker: ticker.clone(),
            };

            let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_TX_OUTPUT_INDEX)
                .with_value(&ticker)?
                .with_value(tx)?
                .with_value(output_index)?;
            received_note_table
                .replace_item_by_unique_multi_index(index_keys, &new_note)
                .await?
        };

        Ok(NoteId::ReceivedNoteId(id.into()))
    }

    pub async fn insert_witness(
        &self,
        note_id: i64,
        witness: &IncrementalWitness<Node>,
        height: BlockHeight,
    ) -> MmResult<(), ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let witness_table = db_transaction.table::<WalletDbSaplingWitnessesTable>().await?;

        let index_keys = MultiIndex::new(WalletDbSaplingWitnessesTable::TICKER_ID_WITNESS_INDEX).with_value(&ticker)?;

        let mut encoded = Vec::new();
        witness.write(&mut encoded).unwrap();

        let note_id_int = BigInt::from_i64(note_id).unwrap();
        let witness = WalletDbSaplingWitnessesTable {
            note: note_id_int,
            block: u32::from(height),
            witness: encoded,
            ticker,
        };
        witness_table
            .replace_item_by_unique_multi_index(index_keys, &witness)
            .await?;

        Ok(())
    }

    pub async fn prune_witnesses(&self, below_height: BlockHeight) -> MmResult<(), ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let witness_table = db_transaction.table::<WalletDbSaplingWitnessesTable>().await?;

        let mut maybe_max_item = witness_table
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .bound("height", 0u32, (below_height - 1).into())
            .reverse()
            .open_cursor(WalletDbBlocksTable::TICKER_HEIGHT_INDEX)
            .await?;

        while let Some((id, _)) = maybe_max_item.next().await? {
            witness_table.delete_item(id).await?;
        }

        Ok(())
    }

    pub async fn update_expired_notes(&self, height: BlockHeight) -> MmResult<(), ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        // fetch received_notes.
        let received_notes_table = db_transaction.table::<WalletDbReceivedNotesTable>().await?;
        let maybe_notes = received_notes_table.get_items("ticker", &ticker).await?;

        // fetch transactions with block < height .
        let txs_table = db_transaction.table::<WalletDbTransactionsTable>().await?;
        let mut maybe_txs = txs_table
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .bound("expiry_height", 0u32, u32::from(height - 1))
            .reverse()
            .open_cursor(WalletDbTransactionsTable::TICKER_EXP_HEIGHT_INDEX)
            .await?;

        while let Some((id, note)) = maybe_txs.next().await? {
            if note.block.is_none() {
                if let Some(curr) = maybe_notes.iter().find(|(_, n)| n.spent == id.to_bigint()) {
                    let temp_note = WalletDbReceivedNotesTable {
                        tx: curr.1.tx,
                        output_index: curr.1.output_index,
                        account: curr.1.account.clone(),
                        diversifier: curr.1.diversifier.clone(),
                        value: curr.1.value.clone(),
                        rcm: curr.1.rcm.clone(),
                        nf: curr.1.nf.clone(),
                        is_change: curr.1.is_change,
                        memo: curr.1.memo.clone(),
                        spent: None,
                        ticker: ticker.clone(),
                    };

                    received_notes_table.replace_item(curr.0, &temp_note).await?;
                }
            };
        }

        Ok(())
    }

    pub async fn get_single_tx(
        &self,
        txid: Vec<u8>,
    ) -> MmResult<Option<(u32, WalletDbTransactionsTable)>, ZcoinStorageError> {
        let ticker = self.ticker.clone();
        let locked_db = self.lock_db().await?;
        let db_transaction = locked_db.get_inner().transaction().await?;
        let walletdb_blocks_table = db_transaction.table::<WalletDbTransactionsTable>().await?;
        let index_keys = MultiIndex::new(WalletDbTransactionsTable::TICKER_TXID_INDEX)
            .with_value(&ticker)?
            .with_value(&txid)?;

        Ok(walletdb_blocks_table.get_item_by_unique_multi_index(index_keys).await?)
    }
}

#[derive(Debug, Clone)]
pub enum NoteId {
    SentNoteId(i64),
    ReceivedNoteId(i64),
}

struct SpendableNoteConstructor {
    diversifier: Vec<u8>,
    value: BigInt,
    rcm: Vec<u8>,
    witness: Vec<u8>,
}

fn to_spendable_note(note: SpendableNoteConstructor) -> MmResult<SpendableNote, ZcoinStorageError> {
    let diversifier = {
        let d = note.diversifier;
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
        let rcm_bytes = &note.rcm;

        // We store rcm directly in the data DB, regardless of whether the note
        // used a v1 or v2 note plaintext, so for the purposes of spending let's
        // pretend this is a pre-ZIP 212 note.
        let rcm = jubjub::Fr::from_repr(
            rcm_bytes[..]
                .try_into()
                .map_to_mm(|_| ZcoinStorageError::InvalidNote("Invalid note".to_string()))?,
        )
        .ok_or_else(|| MmError::new(ZcoinStorageError::InvalidNote("Invalid note".to_string())))?;
        Rseed::BeforeZip212(rcm)
    };

    let witness = {
        let mut d = note.witness.as_slice();
        IncrementalWitness::read(&mut d).map_to_mm(|err| ZcoinStorageError::IoError(err.to_string()))?
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
            .map(|(_, block)| BlockHash::from_slice(&block.hash[..])))
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
            .and_then(|(_, tx)| tx.block.map(BlockHeight::from)))
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
                    .and_then(|k| k.ok_or_else(|| MmError::new(ZcoinStorageError::IncorrectHrpExtFvk)));
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
                    .and_then(|k| k.ok_or_else(|| MmError::new(ZcoinStorageError::IncorrectHrpExtFvk)))?;

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

        // Retrieves a list of transaction IDs (txid) from the transactions table
        // that match the provided account ID.
        let mut txids = vec![];
        while let Some((txid, _tx)) = maybe_txs.next().await? {
            txids.push(txid)
        }

        let received_notes_table = db_transaction.table::<WalletDbReceivedNotesTable>().await?;
        let index_keys = MultiIndex::new(WalletDbReceivedNotesTable::TICKER_ACCOUNT_INDEX)
            .with_value(&ticker)?
            .with_value(account.0)?;
        let maybe_notes = received_notes_table.get_items_by_multi_index(index_keys).await?;

        let mut value: i64 = 0;
        for (_, note) in maybe_notes {
            if txids.contains(&note.tx) && note.spent.is_none() {
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
            return MemoBytes::from_bytes(memo.as_bytes())
                .and_then(Memo::try_from)
                .map_to_mm(|err| ZcoinStorageError::InvalidMemo(err.to_string()));
        };

        MmError::err(ZcoinStorageError::GetFromStorageError("Memo not found".to_string()))
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
            return Ok(Some(
                CommitmentTree::read(&block.sapling_tree[..])
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
            let id_note = block.note.to_i64().unwrap();
            let id_note = NoteId::ReceivedNoteId(id_note);
            let witness = IncrementalWitness::read(block.witness.as_slice())
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
            for (id_tx, tx) in &maybe_txs {
                let id_tx = id_tx.to_bigint();
                if id_tx == note.spent.clone() && tx.block.is_none() {
                    nullifiers.push((
                        AccountId(
                            note.account
                                .to_u32()
                                .ok_or_else(|| ZcoinStorageError::GetFromStorageError("Invalid amount".to_string()))?,
                        ),
                        Nullifier::from_slice(&note.nf.clone().ok_or_else(|| {
                            ZcoinStorageError::GetFromStorageError("Error while putting tx_meta".to_string())
                        })?)
                        .unwrap(),
                    ));
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
        while let Some((id, ts)) = maybe_txs.next().await? {
            txs.push((id, ts))
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
        for (id_note, note) in maybe_notes {
            let id_note = BigInt::from_u32(*id_note).unwrap();
            let witness = witnesses.iter().find(|wit| wit.note == id_note);
            let tx = txs.iter().find(|(id, _tx)| *id == note.tx);

            if let (Some(witness), Some(_)) = (witness, tx) {
                let spend = SpendableNoteConstructor {
                    diversifier: note.diversifier.clone(),
                    value: note.value.clone(),
                    rcm: note.rcm.to_owned(),
                    witness: witness.witness.clone(),
                };
                spendable_notes.push(to_spendable_note(spend)?);
            }
        }

        Ok(spendable_notes)
    }

    async fn select_spendable_notes(
        &self,
        account: AccountId,
        target_value: Amount,
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
        let maybe_notes = maybe_notes
            .clone()
            .into_iter()
            .filter(|(_, note)| note.spent.is_none())
            .collect::<Vec<(u32, WalletDbReceivedNotesTable)>>();

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

        // Sapling Witness
        let witness_table = db_transaction.table::<WalletDbSaplingWitnessesTable>().await?;
        let mut maybe_witness = witness_table
            .cursor_builder()
            .only("ticker", ticker.clone())?
            .bound("block", 0u32, u32::from(anchor_height))
            .open_cursor(WalletDbTransactionsTable::TICKER_BLOCK_INDEX)
            .await?;
        let mut witnesses = vec![];
        while let Some((_, witness)) = maybe_witness.next().await? {
            witnesses.push(witness)
        }

        // Step 1: Calculate the running sum for each note
        let mut running_sum = 0;
        let mut note_running_sums = HashMap::new();

        for (id_note, note) in maybe_notes.iter() {
            if note.account == account.0.into() {
                let value = note.value.clone().to_i64().expect("price is too large");
                running_sum += value;
            }

            note_running_sums.insert(id_note, running_sum);
        }

        // Step 2: Select eligible notes
        let mut selected_notes = Vec::new();
        for (id_note, note) in maybe_notes.iter() {
            if note.account == account.0.into() && note.spent.is_none() {
                let note_running_sum = note_running_sums.get(&id_note).unwrap_or(&0);
                if Amount::from_i64(*note_running_sum)
                    .map_to_mm(|_| ZcoinStorageError::CorruptedData("price is too large".to_string()))?
                    < target_value
                {
                    selected_notes.push((id_note, note, *note_running_sum));
                }
            }
        }

        // Step 2: Select all unspent notes in the desired account, along with their running sum.
        let mut final_notes = Vec::new();
        for (id_note, note, sum) in &selected_notes {
            if note.spent.is_none()
                && Amount::from_i64(*sum)
                    .map_to_mm(|_| ZcoinStorageError::CorruptedData("price is too large".to_string()))?
                    < target_value
            {
                final_notes.push((id_note, note));
            }
        }

        // Step 4: Get witnesses for selected notes
        let mut spendable_notes = Vec::new();
        for (id_note, note) in final_notes.iter() {
            let noteid_bigint = BigInt::from_u32(***id_note).unwrap();
            if let Some(witness) = witnesses.iter().find(|&w| w.note == noteid_bigint) {
                spendable_notes.push(to_spendable_note(SpendableNoteConstructor {
                    diversifier: note.diversifier.clone(),
                    value: note.value.clone(),
                    rcm: note.rcm.clone(),
                    witness: witness.witness.clone(),
                })?);
            }
        }

        Ok(spendable_notes)
    }
}

#[async_trait]
impl WalletWrite for WalletIndexedDb {
    async fn advance_by_block(
        &mut self,
        block: &PrunedBlock,
        updated_witnesses: &[(Self::NoteRef, IncrementalWitness<Node>)],
    ) -> Result<Vec<(Self::NoteRef, IncrementalWitness<Node>)>, Self::Error> {
        let selfi = self.deref();
        selfi
            .insert_block(
                block.block_height,
                block.block_hash,
                block.block_time,
                block.commitment_tree,
            )
            .await?;

        let mut new_witnesses = vec![];
        for tx in block.transactions {
            let tx_row = selfi.put_tx_meta(tx, block.block_height).await?;

            // Mark notes as spent and remove them from the scanning cache
            for spend in &tx.shielded_spends {
                selfi.mark_spent(tx_row, &spend.nf).await?;
            }

            for output in &tx.shielded_outputs {
                let received_note_id = selfi.put_received_note(output, tx_row).await?;

                // Save witness for note.
                new_witnesses.push((received_note_id, output.witness.clone()));
            }
        }

        // Insert current new_witnesses into the database.
        for (received_note_id, witness) in updated_witnesses.iter().chain(new_witnesses.iter()) {
            if let NoteId::ReceivedNoteId(rnid) = *received_note_id {
                selfi.insert_witness(rnid, witness, block.block_height).await?;
            } else {
                return MmError::err(ZcoinStorageError::InvalidNoteId);
            }
        }

        // Prune the stored witnesses (we only expect rollbacks of at most 100 blocks).
        let below_height = if block.block_height < BlockHeight::from(100) {
            BlockHeight::from(0)
        } else {
            block.block_height - 100
        };
        selfi.prune_witnesses(below_height).await?;

        // Update now-expired transactions that didn't get mined.
        selfi.update_expired_notes(block.block_height).await?;

        Ok(new_witnesses)
    }

    async fn store_received_tx(&mut self, _received_tx: &ReceivedTransaction) -> Result<Self::TxRef, Self::Error> {
        //        let selfi = self.deref();
        //        let tx_ref = selfi.put_tx_data(received_tx.tx, None).await?;
        //
        //        for output in received_tx.outputs {
        //            if output.outgoing {
        //                selfi.put_sent_note(output, tx_ref).await?;
        //            } else {
        //                selfi.put_received_note(output, tx_ref).await?;
        //            }
        //        }
        //
        //        Ok(tx_ref)

        todo!()
    }

    async fn store_sent_tx(&mut self, _sent_tx: &SentTransaction) -> Result<Self::TxRef, Self::Error> { todo!() }

    async fn rewind_to_height(&mut self, _block_height: BlockHeight) -> Result<(), Self::Error> { todo!() }
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
    const TABLE_NAME: &'static str = "walletdb_accounts";

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::TABLE_NAME)?;
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
    hash: Vec<u8>,
    time: u32,
    sapling_tree: Vec<u8>,
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
    const TABLE_NAME: &'static str = "walletdb_blocks";

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::TABLE_NAME)?;
            table.create_multi_index(Self::TICKER_HEIGHT_INDEX, &["ticker", "height"], true)?;
            table.create_multi_index(Self::TICKER_HASH_INDEX, &["ticker", "hash"], true)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbTransactionsTable {
    txid: Vec<u8>, // unique
    created: Option<String>,
    block: Option<u32>,
    tx_index: Option<i64>,
    expiry_height: Option<u32>,
    raw: Option<Vec<u8>>,
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
    pub const TICKER_EXP_HEIGHT_INDEX: &'static str = "ticker_expiry_height_index";
}

impl TableSignature for WalletDbTransactionsTable {
    const TABLE_NAME: &'static str = "walletdb_transactions";

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::TABLE_NAME)?;
            table.create_multi_index(Self::TICKER_TXID_INDEX, &["ticker", "txid"], true)?;
            table.create_multi_index(Self::TICKER_BLOCK_INDEX, &["ticker", "block"], false)?;
            table.create_multi_index(Self::TICKER_EXP_HEIGHT_INDEX, &["ticker", "expiry_height"], false)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbReceivedNotesTable {
    // references transactions(id_tx)
    tx: u32,
    output_index: u32,
    // references accounts(account)
    account: BigInt,
    diversifier: Vec<u8>,
    value: BigInt,
    rcm: Vec<u8>,
    nf: Option<Vec<u8>>, // unique
    is_change: Option<bool>,
    memo: Option<Vec<u8>>,
    // references transactions(id_tx)
    spent: Option<BigInt>,
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
    pub const TICKER_NF_INDEX: &'static str = "ticker_nf_index";
    pub const TICKER_TX_OUTPUT_INDEX: &'static str = "ticker_tx_output_index";
}

impl TableSignature for WalletDbReceivedNotesTable {
    const TABLE_NAME: &'static str = "walletdb_received_notes";

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::TABLE_NAME)?;
            table.create_multi_index(Self::TICKER_ID_NOTE_INDEX, &["ticker", "id_note"], true)?;
            table.create_multi_index(
                Self::TICKER_NOTES_TX_OUTPUT_INDEX,
                &["ticker", "tx", "output_index"],
                true,
            )?;
            table.create_multi_index(Self::TICKER_ACCOUNT_INDEX, &["ticker", "account"], false)?;
            table.create_multi_index(Self::TICKER_NF_INDEX, &["ticker", "nf"], false)?;
            table.create_multi_index(Self::TICKER_TX_OUTPUT_INDEX, &["ticker", "tx", "output_index"], false)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletDbSaplingWitnessesTable {
    //    id_witness: u32,
    // REFERENCES received_notes(id_note)
    note: BigInt,
    // REFERENCES blocks(height)
    block: u32,
    witness: Vec<u8>,
    ticker: String,
}

impl WalletDbSaplingWitnessesTable {
    /// A **unique** index that consists of the following properties:
    /// * ticker
    /// * note
    /// * block
    pub const TICKER_NOTE_BLOCK_INDEX: &'static str = "ticker_note_block_index";
    pub const TICKER_BLOCK_INDEX: &'static str = "ticker_block_index";
    pub const TICKER_ID_WITNESS_INDEX: &'static str = "ticker_witness_index";
}

impl TableSignature for WalletDbSaplingWitnessesTable {
    const TABLE_NAME: &'static str = "walletdb_sapling_witness";

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::TABLE_NAME)?;
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
    const TABLE_NAME: &'static str = "walletdb_sent_notes";

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if let (0, 1) = (old_version, new_version) {
            let table = upgrader.create_table(Self::TABLE_NAME)?;
            table.create_multi_index(Self::TICKER_TX_OUTPUT_INDEX, &["ticker", "tx", "output_index"], true)?;
            table.create_multi_index(Self::TICKER_ID_NOTE_INDEX, &["ticker", "id_note"], true)?;
            table.create_index("ticker", false)?;
        }
        Ok(())
    }
}
