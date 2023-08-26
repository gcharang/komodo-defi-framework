pub mod blockdb;

pub use blockdb::*;
use std::fmt::Debug;
use std::sync::Arc;

pub mod walletdb;
pub use walletdb::*;

use async_trait::async_trait;
use common::block_on;
use futures::lock::Mutex;
use zcash_client_backend::data_api::error::{ChainInvalid, Error as ChainError};
use zcash_client_backend::data_api::PrunedBlock;
use zcash_client_backend::proto::compact_formats::CompactBlock;
use zcash_client_backend::wallet::{AccountId, WalletTx};
use zcash_client_backend::welding_rig::scan_block;
use zcash_client_sqlite::with_async::WalletWrite;
use zcash_primitives::block::BlockHash;
use zcash_primitives::consensus;
use zcash_primitives::consensus::{BlockHeight, NetworkUpgrade};
use zcash_primitives::merkle_tree::CommitmentTree;
use zcash_primitives::sapling::Nullifier;
use zcash_primitives::zip32::ExtendedFullViewingKey;

/// This trait provides sequential access to raw blockchain data via a callback-oriented
/// API.
#[async_trait]
pub trait BlockSource {
    type Error;

    /// Scan the specified `limit` number of blocks from the blockchain, starting at
    /// `from_height`, applying the provided callback to each block.
    async fn with_blocks<F>(
        &self,
        from_height: BlockHeight,
        limit: Option<u32>,
        with_row: F,
    ) -> Result<(), Self::Error>
    where
        F: FnMut(CompactBlock) -> Result<(), Self::Error> + Send;
}

pub async fn validate_chain<'a, N, E, P, C>(
    parameters: &P,
    cache: &C,
    validate_from: Option<(BlockHeight, BlockHash)>,
) -> Result<(), E>
where
    E: From<ChainError<N>> + Send + 'a,
    P: consensus::Parameters,
    C: BlockSource<Error = E>,
{
    let sapling_activation_height = parameters
        .activation_height(NetworkUpgrade::Sapling)
        .ok_or(ChainError::SaplingNotActive)?;

    // The cache will contain blocks above the `validate_from` height.  Validate from that maximum
    // height up to the chain tip, returning the hash of the block found in the cache at the
    // `validate_from` height, which can then be used to verify chain integrity by comparing
    // against the `validate_from` hash.
    let from_height = validate_from
        .map(|(height, _)| height)
        .unwrap_or(sapling_activation_height - 1);

    let mut prev_height = from_height;
    let mut prev_hash: Option<BlockHash> = validate_from.map(|(_, hash)| hash);

    cache
        .with_blocks(from_height, None, |block: CompactBlock| {
            let current_height = block.height();
            let result = if current_height != prev_height + 1 {
                Err(ChainInvalid::block_height_discontinuity(
                    prev_height + 1,
                    current_height,
                ))
            } else {
                match prev_hash {
                    None => Ok(()),
                    Some(h) if h == block.prev_hash() => Ok(()),
                    Some(_) => Err(ChainInvalid::prev_hash_mismatch(current_height)),
                }
            };

            prev_height = current_height;
            prev_hash = Some(block.hash());
            result.map_err(E::from)
        })
        .await?;

    Ok(())
}

pub async fn scan_cached_blocks<'a, E, N, P, C, D>(
    params: &P,
    cache: &C,
    data: Arc<Mutex<D>>,
    limit: Option<u32>,
) -> Result<(), E>
where
    P: consensus::Parameters + Send + Sync,
    C: BlockSource<Error = E>,
    D: WalletWrite<Error = E, NoteRef = N>,
    N: Copy + Debug + Send,
    E: From<ChainError<N>> + Send + 'a,
{
    let mut data_guard = data.lock().await;
    let sapling_activation_height = params
        .activation_height(NetworkUpgrade::Sapling)
        .ok_or(ChainError::SaplingNotActive)?;

    // Recall where we synced up to previously.
    // If we have never synced, use sapling activation height to select all cached CompactBlocks.
    let mut last_height = data_guard
        .block_height_extrema()
        .await
        .map(|opt| opt.map(|(_, max)| max).unwrap_or(sapling_activation_height - 1))?;

    // Fetch the ExtendedFullViewingKeys we are tracking
    let extfvks = data_guard.get_extended_full_viewing_keys().await?;
    let extfvks: Vec<(&AccountId, &ExtendedFullViewingKey)> = extfvks.iter().collect();

    // Get the most recent CommitmentTree
    let mut tree = data_guard
        .get_commitment_tree(last_height)
        .await
        .map(|t| t.unwrap_or_else(CommitmentTree::empty))?;

    // Get most recent incremental witnesses for the notes we are tracking
    let mut witnesses = data_guard.get_witnesses(last_height).await?;

    // Get the nullifiers for the notes we are tracking
    let mut nullifiers = data_guard.get_nullifiers().await?;

    cache
        .with_blocks(
            last_height,
            limit,
            Box::new(|block: CompactBlock| {
                let current_height = block.height();

                // Scanned blocks MUST be height-sequential.
                if current_height != (last_height + 1) {
                    return Err(ChainInvalid::block_height_discontinuity(last_height + 1, current_height).into());
                }

                let block_hash = BlockHash::from_slice(&block.hash);
                let block_time = block.time;

                let txs: Vec<WalletTx<Nullifier>> = {
                    let mut witness_refs: Vec<_> = witnesses.iter_mut().map(|w| &mut w.1).collect();

                    scan_block(params, block, &extfvks, &nullifiers, &mut tree, &mut witness_refs[..])
                };

                // Enforce that all roots match. This is slow, so only include in debug builds.
                #[cfg(debug_assertions)]
                {
                    let cur_root = tree.root();
                    for row in &witnesses {
                        if row.1.root() != cur_root {
                            return Err(ChainError::InvalidWitnessAnchor(row.0, current_height).into());
                        }
                    }
                    for tx in &txs {
                        for output in tx.shielded_outputs.iter() {
                            if output.witness.root() != cur_root {
                                return Err(ChainError::InvalidNewWitnessAnchor(
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

                let new_witnesses = block_on(data_guard.advance_by_block(
                    &(PrunedBlock {
                        block_height: current_height,
                        block_hash,
                        block_time,
                        commitment_tree: &tree,
                        transactions: &txs,
                    }),
                    &witnesses,
                ))?;

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

                last_height = current_height;

                Ok(())
            }),
        )
        .await
}
