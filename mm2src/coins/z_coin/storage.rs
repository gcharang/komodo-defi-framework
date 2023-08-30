use crate::z_coin::ZcoinConsensusParams;

pub mod blockdb;
pub use blockdb::*;

pub mod walletdb;
pub use walletdb::*;

#[cfg(target_arch = "wasm32")]
use walletdb::wallet_idb_storage::DataConnStmtCacheWasm;
use zcash_client_backend::data_api::error::ChainInvalid;
#[cfg(debug_assertions)]
use zcash_client_backend::data_api::error::Error;
use zcash_client_backend::proto::compact_formats::CompactBlock;
#[cfg(not(target_arch = "wasm32"))]
use zcash_client_sqlite::error::SqliteClientError;
#[cfg(not(target_arch = "wasm32"))]
use zcash_client_sqlite::with_async::DataConnStmtCacheAsync;
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::BlockHeight;

cfg_native!(
    use zcash_client_backend::data_api::PrunedBlock;
    use zcash_client_backend::wallet::{AccountId, WalletTx};
    use zcash_client_backend::welding_rig::scan_block;
    use zcash_extras::{WalletRead, WalletWrite};
    use zcash_primitives::merkle_tree::CommitmentTree;
    use zcash_primitives::sapling::Nullifier;
    use zcash_primitives::zip32::ExtendedFullViewingKey;
);

#[derive(Clone)]
pub struct DataConnStmtCacheWrapper {
    #[cfg(not(target_arch = "wasm32"))]
    pub cache: DataConnStmtCacheAsync<ZcoinConsensusParams>,
    #[cfg(target_arch = "wasm32")]
    pub cache: DataConnStmtCacheWasm,
}

impl DataConnStmtCacheWrapper {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(cache: DataConnStmtCacheAsync<ZcoinConsensusParams>) -> Self { Self { cache } }
    #[cfg(target_arch = "wasm32")]
    pub fn new(cache: DataConnStmtCacheWasm) -> Self { Self { cache } }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn inner(&self) -> DataConnStmtCacheAsync<ZcoinConsensusParams> { self.clone().cache }
    #[cfg(target_arch = "wasm32")]
    pub fn inner(&self) -> DataConnStmtCacheWasm { self.clone().cache }
}

#[allow(unused)]
pub struct CompactBlockRow {
    pub(crate) height: BlockHeight,
    pub(crate) data: Vec<u8>,
}

#[derive(Clone)]
pub enum BlockProcessingMode {
    Validate,
    Scan(DataConnStmtCacheWrapper),
}

#[derive(Debug, Display)]
pub enum ValidateBlocksError {
    #[display(fmt = "Chain Invalid occurred at height: {height:?} — with error {err:?}")]
    ChainInvalid {
        height: BlockHeight,
        err: ChainInvalid,
    },
    GetFromStorageError(String),
    IoError(String),
    DbError(String),
    DecodingError(String),
    TableNotEmpty(String),
    InvalidNote(String),
    InvalidNoteId(String),
    IncorrectHrpExtFvk(String),
    CorruptedData(String),
    InvalidMemo(String),
    BackendError(String),
}

impl ValidateBlocksError {
    pub fn prev_hash_mismatch(height: BlockHeight) -> ValidateBlocksError {
        ValidateBlocksError::ChainInvalid {
            height,
            err: ChainInvalid::PrevHashMismatch,
        }
    }

    pub fn block_height_discontinuity(height: BlockHeight, found: BlockHeight) -> ValidateBlocksError {
        ValidateBlocksError::ChainInvalid {
            height,
            err: ChainInvalid::BlockHeightDiscontinuity(found),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<SqliteClientError> for ValidateBlocksError {
    fn from(value: SqliteClientError) -> Self {
        match value {
            SqliteClientError::CorruptedData(err) => Self::CorruptedData(err),
            SqliteClientError::IncorrectHrpExtFvk => Self::IncorrectHrpExtFvk(value.to_string()),
            SqliteClientError::InvalidNote => Self::InvalidNote(value.to_string()),
            SqliteClientError::InvalidNoteId => Self::InvalidNoteId(value.to_string()),
            SqliteClientError::TableNotEmpty => Self::TableNotEmpty(value.to_string()),
            SqliteClientError::Bech32(_) | SqliteClientError::Base58(_) => Self::DecodingError(value.to_string()),
            SqliteClientError::DbError(err) => Self::DbError(err.to_string()),
            SqliteClientError::Io(err) => Self::IoError(err.to_string()),
            SqliteClientError::InvalidMemo(err) => Self::InvalidMemo(err.to_string()),
            SqliteClientError::BackendError(err) => Self::BackendError(err.to_string()),
        }
    }
}

/// Checks that the scanned blocks in the data database, when combined with the recent
/// `CompactBlock`s in the cache database, form a valid chain.
///
/// This function is built on the core assumption that the information provided in the
/// cache database is more likely to be accurate than the previously-scanned information.
/// This follows from the design (and trust) assumption that the `lightwalletd` server
/// provides accurate block information as of the time it was requested.
///
pub async fn validate_chain(
    block: CompactBlock,
    prev_height: &mut BlockHeight,
    prev_hash: &mut Option<BlockHash>,
) -> Result<(), ValidateBlocksError> {
    let current_height = block.height();
    if current_height != *prev_height + 1 {
        Err(ValidateBlocksError::block_height_discontinuity(
            *prev_height + 1,
            current_height,
        ))
    } else {
        match prev_hash {
            None => Ok(()),
            Some(ref h) if h == &block.prev_hash() => Ok(()),
            Some(_) => Err(ValidateBlocksError::prev_hash_mismatch(current_height)),
        }
    }?;

    *prev_height = current_height;
    *prev_hash = Some(block.hash());

    Ok(())
}

/// Scans at most `limit` new blocks added to the cache for any transactions received by
/// the tracked accounts.
///
/// This function will return without error after scanning at most `limit` new blocks, to
/// enable the caller to update their UI with scanning progress. Repeatedly calling this
/// function will process sequential ranges of blocks, and is equivalent to calling
/// `scan_cached_blocks` and passing `None` for the optional `limit` value.
///
/// This function pays attention only to cached blocks with heights greater than the
/// highest scanned block in `data`. Cached blocks with lower heights are not verified
/// against previously-scanned blocks. In particular, this function **assumes** that the
/// caller is handling rollbacks.
///
/// For brand-new light client databases, this function starts scanning from the Sapling
/// activation height. This height can be fast-forwarded to a more recent block by
/// initializing the client database with a starting block (for example, calling
/// `init_blocks_table` before this function if using `zcash_client_sqlite`).
///
/// Scanned blocks are required to be height-sequential. If a block is missing from the
/// cache, an error will be returned with kind [`ChainInvalid::BlockHeightDiscontinuity`].
///
#[cfg(not(target_arch = "wasm32"))]
pub async fn scan_cached_block(
    data: DataConnStmtCacheWrapper,
    params: &ZcoinConsensusParams,
    block: &CompactBlock,
    last_height: &mut BlockHeight,
) -> Result<(), ValidateBlocksError> {
    let mut data_guard = data.inner();
    // Fetch the ExtendedFullViewingKeys we are tracking
    let extfvks = data_guard.get_extended_full_viewing_keys().await?;
    let extfvks: Vec<(&AccountId, &ExtendedFullViewingKey)> = extfvks.iter().collect();

    // Get the most recent CommitmentTree
    let mut tree = data_guard
        .get_commitment_tree(*last_height)
        .await
        .map(|t| t.unwrap_or_else(CommitmentTree::empty))?;

    // Get most recent incremental witnesses for the notes we are tracking
    let mut witnesses = data_guard.get_witnesses(*last_height).await?;

    // Get the nullifiers for the notes we are tracking
    let mut nullifiers = data_guard.get_nullifiers().await?;

    let current_height = block.height();
    // Scanned blocks MUST be height-sequential.
    if current_height != (*last_height + 1) {
        return Err(ValidateBlocksError::block_height_discontinuity(
            *last_height + 1,
            current_height,
        ));
    }

    let block_hash = BlockHash::from_slice(&block.hash);
    let block_time = block.time;

    let txs: Vec<WalletTx<Nullifier>> = {
        let mut witness_refs: Vec<_> = witnesses.iter_mut().map(|w| &mut w.1).collect();
        scan_block(
            params,
            block.clone(),
            &extfvks,
            &nullifiers,
            &mut tree,
            &mut witness_refs[..],
        )
    };

    // Enforce that all roots match. This is slow, so only include in debug builds.
    #[cfg(debug_assertions)]
    {
        let cur_root = tree.root();
        for row in &witnesses {
            if row.1.root() != cur_root {
                return Err(Error::InvalidWitnessAnchor(row.0, current_height).into());
            }
        }
        for tx in &txs {
            for output in tx.shielded_outputs.iter() {
                if output.witness.root() != cur_root {
                    return Err(Error::InvalidNewWitnessAnchor(
                        output.index,
                        tx.txid,
                        current_height,
                        output.witness.root(),
                    )
                    .into());
                }
            }
        }
    }

    let new_witnesses = data_guard
        .advance_by_block(
            &(PrunedBlock {
                block_height: current_height,
                block_hash,
                block_time,
                commitment_tree: &tree,
                transactions: &txs,
            }),
            &witnesses,
        )
        .await?;

    let spent_nf: Vec<Nullifier> = txs
        .iter()
        .flat_map(|tx| tx.shielded_spends.iter().map(|spend| spend.nf))
        .collect();
    nullifiers.retain(|(_, nf)| !spent_nf.contains(nf));
    nullifiers.extend(
        txs.iter()
            .flat_map(|tx| tx.shielded_outputs.iter().map(|out| (out.account, out.nf))),
    );

    witnesses.extend(new_witnesses);

    *last_height = current_height;

    Ok(())
}

#[allow(unused)]
#[derive(Debug, Display)]
pub enum ZcoinStorageError {
    #[cfg(not(target_arch = "wasm32"))]
    SqliteError(SqliteClientError),
    ValidateBlocksError(ValidateBlocksError),
    #[display(fmt = "Chain Invalid occurred at height: {height:?} — with error {err:?}")]
    ChainInvalid {
        height: BlockHeight,
        err: ChainInvalid,
    },
    IoError(String),
    DbError(String),
    DecodingError(String),
    TableNotEmpty(String),
    InvalidNote(String),
    InvalidNoteId(String),
    #[display(fmt = "Incorrect Hrp extended full viewing key")]
    IncorrectHrpExtFvk,
    CorruptedData(String),
    InvalidMemo(String),
    BackendError(String),
    #[display(fmt = "Add to storage err: {}", _0)]
    AddToStorageErr(String),
    #[display(fmt = "Remove from storage err: {}", _0)]
    RemoveFromStorageErr(String),
    #[display(fmt = "Get from storage err: {}", _0)]
    GetFromStorageError(String),
    #[display(fmt = "Error getting {ticker} block height from storage: {err}")]
    BlockHeightNotFound {
        ticker: String,
        err: String,
    },
    #[display(fmt = "Storage Initialization err: {err} - ticker: {ticker}")]
    InitDbError {
        ticker: String,
        err: String,
    },
    #[cfg(target_arch = "wasm32")]
    #[display(fmt = "IndexedDB table err: {err} - ticker: {ticker}")]
    IdbTableError {
        ticker: String,
        err: String,
    },
    ChainError(String),
    InternalError(String),
    NotSupported(String),
}

#[cfg(target_arch = "wasm32")]
use mm2_db::indexed_db::{CursorError, DbTransactionError, InitDbError};

#[cfg(target_arch = "wasm32")]
impl From<InitDbError> for ZcoinStorageError {
    fn from(e: InitDbError) -> Self {
        match &e {
            InitDbError::NotSupported(_) => ZcoinStorageError::NotSupported(e.to_string()),
            InitDbError::EmptyTableList
            | InitDbError::DbIsOpenAlready { .. }
            | InitDbError::InvalidVersion(_)
            | InitDbError::OpeningError(_)
            | InitDbError::TypeMismatch { .. }
            | InitDbError::UnexpectedState(_)
            | InitDbError::UpgradingError { .. } => ZcoinStorageError::InternalError(e.to_string()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl From<DbTransactionError> for ZcoinStorageError {
    fn from(e: DbTransactionError) -> Self {
        match e {
            DbTransactionError::ErrorSerializingItem(_) | DbTransactionError::ErrorDeserializingItem(_) => {
                ZcoinStorageError::DecodingError(e.to_string())
            },
            DbTransactionError::ErrorUploadingItem(_) => ZcoinStorageError::AddToStorageErr(e.to_string()),
            DbTransactionError::ErrorGettingItems(_) | DbTransactionError::ErrorCountingItems(_) => {
                ZcoinStorageError::GetFromStorageError(e.to_string())
            },
            DbTransactionError::ErrorDeletingItems(_) => ZcoinStorageError::RemoveFromStorageErr(e.to_string()),
            DbTransactionError::NoSuchTable { .. }
            | DbTransactionError::ErrorCreatingTransaction(_)
            | DbTransactionError::ErrorOpeningTable { .. }
            | DbTransactionError::ErrorSerializingIndex { .. }
            | DbTransactionError::UnexpectedState(_)
            | DbTransactionError::TransactionAborted
            | DbTransactionError::MultipleItemsByUniqueIndex { .. }
            | DbTransactionError::NoSuchIndex { .. }
            | DbTransactionError::InvalidIndex { .. } => ZcoinStorageError::InternalError(e.to_string()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl From<CursorError> for ZcoinStorageError {
    fn from(value: CursorError) -> Self {
        match value {
            CursorError::ErrorSerializingIndexFieldValue { .. }
            | CursorError::ErrorDeserializingIndexValue { .. }
            | CursorError::ErrorDeserializingItem(_) => Self::DecodingError(value.to_string()),
            CursorError::ErrorOpeningCursor { .. }
            | CursorError::AdvanceError { .. }
            | CursorError::InvalidKeyRange { .. }
            | CursorError::IncorrectNumberOfKeysPerIndex { .. }
            | CursorError::UnexpectedState(_)
            | CursorError::IncorrectUsage { .. }
            | CursorError::TypeMismatch { .. } => Self::DbError(value.to_string()),
        }
    }
}
