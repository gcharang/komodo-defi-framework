pub mod blockdb;

pub use blockdb::*;

pub mod walletdb;
pub use walletdb::*;

use crate::z_coin::ZcoinConsensusParams;
use zcash_client_backend::data_api::error::ChainInvalid;
#[cfg(debug_assertions)]
use zcash_client_backend::data_api::error::Error;
use zcash_client_backend::data_api::PrunedBlock;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_client_backend::wallet::{AccountId, WalletTx};
use zcash_client_backend::welding_rig::scan_block;
use zcash_client_sqlite::error::SqliteClientError;
use zcash_client_sqlite::with_async::DataConnStmtCacheAsync;
use zcash_extras::{WalletRead, WalletWrite};
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus::BlockHeight;
use zcash_primitives::merkle_tree::CommitmentTree;
use zcash_primitives::sapling::Nullifier;
use zcash_primitives::zip32::ExtendedFullViewingKey;

#[derive(Clone)]
pub enum BlockProcessingMode {
    Validate,
    Scan(DataConnStmtCacheAsync<ZcoinConsensusParams>),
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
) -> Result<(), SqliteClientError> {
    let current_height = block.height();
    if current_height != *prev_height + 1 {
        Err(ChainInvalid::block_height_discontinuity(
            *prev_height + 1,
            current_height,
        ))
    } else {
        match prev_hash {
            None => Ok(()),
            Some(ref h) if h == &block.prev_hash() => Ok(()),
            Some(_) => Err(ChainInvalid::prev_hash_mismatch(current_height)),
        }
    }
    .map_err(SqliteClientError::from)?;

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
pub async fn scan_cached_block(
    data: DataConnStmtCacheAsync<ZcoinConsensusParams>,
    params: &ZcoinConsensusParams,
    block: &CompactBlock,
    last_height: &mut BlockHeight,
) -> Result<(), SqliteClientError> {
    let mut data_guard = data;
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
        return Err(ChainInvalid::block_height_discontinuity(*last_height + 1, current_height).into());
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
