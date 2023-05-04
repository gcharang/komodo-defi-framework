use crate::nft::nft_structs::{Chain, ConvertChain, Nft, NftList, NftTransferHistory, NftTxHistoryFilters,
                              NftsTransferHistoryList};
use crate::nft_storage::{CreateNftStorageError, NftListStorageOps, NftStorageError, NftTxHistoryStorageOps};
use async_trait::async_trait;
use common::async_blocking;
use db_common::sql_build::SqlQuery;
use db_common::sqlite::rusqlite::types::Type;
use db_common::sqlite::rusqlite::{Connection, Error as SqlError, Row, NO_PARAMS};
use db_common::sqlite::{query_single_row, string_from_row, validate_table_name, CHECK_TABLE_EXISTS_SQL};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::mm_error::{MmError, MmResult};
use mm2_err_handle::or_mm_error::OrMmError;
use mm2_number::BigDecimal;
use serde_json::{self as json};
use std::convert::TryInto;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

fn nft_list_table_name(chain: &Chain) -> String { chain.to_ticker() + "_nft_list" }

fn nft_tx_history_table_name(chain: &Chain) -> String { chain.to_ticker() + "_nft_tx_history" }

fn create_nft_list_table_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_list_table_name(chain);
    validate_table_name(&table_name)?;
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (
    token_address VARCHAR(256) NOT NULL,
    token_id VARCHAR(256) NOT NULL,
    chain TEXT NOT NULL,
    amount VARCHAR(256) NOT NULL,
    owner_of TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    block_number_minted INTEGER NOT NULL,
    block_number INTEGER NOT NULL,
    contract_type TEXT,
    name TEXT,
    symbol TEXT,
    token_uri TEXT,
    metadata BLOB,
    last_token_uri_sync TEXT,
    last_metadata_sync TEXT,
    minter_address TEXT,
    possible_spam INTEGER,
    details_json TEXT,
    PRIMARY KEY (token_address, token_id)
        );",
        table_name
    );
    Ok(sql)
}

fn create_tx_history_table_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_tx_history_table_name(chain);
    validate_table_name(&table_name)?;
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (
    transaction_hash VARCHAR(256) PRIMARY KEY,
    chain TEXT NOT NULL,
    block_number INTEGER NOT NULL,
    block_timestamp TEXT NOT NULL,
    block_hash TEXT NOT NULL,
    transaction_index INTEGER NOT NULL,
    log_index INTEGER NOT NULL,
    value VARCHAR(256) NOT NULL,
    contract_type TEXT NOT NULL,
    transaction_type TEXT NOT NULL,
    token_address VARCHAR(256) NOT NULL,
    token_id VARCHAR(256) NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    amount VARCHAR(256) NOT NULL,
    verified INTEGER NOT NULL,
    operator TEXT,
    possible_spam INTEGER,
    details_json TEXT
        );",
        table_name
    );
    Ok(sql)
}

impl NftStorageError for SqlError {}

#[derive(Clone)]
pub struct SqliteNftStorage(Arc<Mutex<Connection>>);

impl SqliteNftStorage {
    pub fn new(ctx: &MmArc) -> MmResult<Self, CreateNftStorageError> {
        let sqlite_connection = ctx
            .sqlite_connection
            .ok_or(MmError::new(CreateNftStorageError::Internal(
                "sqlite_connection is not initialized".to_owned(),
            )))?;
        Ok(SqliteNftStorage(sqlite_connection.clone()))
    }
}

fn get_nft_list_builder_preimage(conn: &Connection, chains: Vec<Chain>) -> MmResult<SqlQuery, SqlError> {
    let union_sql_strings: MmResult<Vec<_>, SqlError> = chains
        .iter()
        .map(|chain| {
            let table_name = nft_list_table_name(chain);
            validate_table_name(&table_name)?;
            let sql_builder = nft_table_builder_preimage(conn, table_name.as_str())?;
            let sql_string = sql_builder.sql()?.trim_end_matches(';').to_string();
            Ok(sql_string)
        })
        .collect();

    let union_sql_strings = union_sql_strings?;
    let union_sql = union_sql_strings.join(" UNION ALL ");
    let final_sql_builder = SqlQuery::select_from_union_alias(conn, union_sql.as_str(), "nft_list")?;
    Ok(final_sql_builder)
}

// todo impl filters
fn get_nft_tx_builder_preimage(
    conn: &Connection,
    chains: Vec<Chain>,
    _filters: Option<NftTxHistoryFilters>,
) -> MmResult<SqlQuery, SqlError> {
    let union_sql_strings: MmResult<Vec<_>, SqlError> = chains
        .iter()
        .map(|chain| {
            let table_name = nft_tx_history_table_name(chain);
            validate_table_name(&table_name)?;
            // todo here add filters
            let sql_builder = nft_table_builder_preimage(conn, table_name.as_str())?;
            let sql_string = sql_builder.sql()?.trim_end_matches(';').to_string();
            Ok(sql_string)
        })
        .collect();

    let union_sql_strings = union_sql_strings?;
    let union_sql = union_sql_strings.join(" UNION ALL ");
    let final_sql_builder = SqlQuery::select_from_union_alias(conn, union_sql.as_str(), "nft_history")?;
    Ok(final_sql_builder)
}

fn nft_table_builder_preimage<'a>(conn: &'a Connection, table_name: &'a str) -> MmResult<SqlQuery<'a>, SqlError> {
    let sql_builder = SqlQuery::select_from(conn, table_name)?;
    Ok(sql_builder)
}

fn finalize_nft_list_sql_builder(sql_builder: &mut SqlQuery, offset: usize, limit: usize) -> MmResult<(), SqlError> {
    sql_builder.field("nft_list.details_json")?.offset(offset).limit(limit);
    Ok(())
}

fn finalize_nft_history_sql_builder(sql_builder: &mut SqlQuery, offset: usize, limit: usize) -> MmResult<(), SqlError> {
    sql_builder
        .field("nft_history.details_json")?
        .offset(offset)
        .limit(limit);
    Ok(())
}

fn nft_from_row(row: &Row<'_>) -> Result<Nft, SqlError> {
    let json_string: String = row.get(0)?;
    json::from_str(&json_string).map_err(|e| SqlError::FromSqlConversionFailure(0, Type::Text, Box::new(e)))
}

fn tx_history_from_row(row: &Row<'_>) -> Result<NftTransferHistory, SqlError> {
    let json_string: String = row.get(0)?;
    json::from_str(&json_string).map_err(|e| SqlError::FromSqlConversionFailure(0, Type::Text, Box::new(e)))
}

fn insert_nft_in_list_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_list_table_name(chain);
    validate_table_name(&table_name)?;

    let sql = format!(
        "INSERT OR IGNORE INTO {} (
            chain, token_address, token_id, amount, owner_of, token_hash,
            block_number_minted, block_number, contract_type, name, symbol,
            token_uri, metadata, last_token_uri_sync, last_metadata_sync, minter_address, possible_spam,
            details_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18
        );",
        table_name
    );
    Ok(sql)
}

fn insert_tx_in_history_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_tx_history_table_name(chain);
    validate_table_name(&table_name)?;

    let sql = format!(
        "INSERT OR IGNORE INTO {} (
            transaction_hash, chain, block_number, block_timestamp, block_hash,
            transaction_index, log_index, value, contract_type, transaction_type,
            token_address, token_id, from_address, to_address, amount, verified,
            operator, possible_spam, details_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19
        );",
        table_name
    );
    Ok(sql)
}

#[async_trait]
impl NftListStorageOps for SqliteNftStorage {
    type Error = SqlError;

    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        let selfi = self.clone();
        let sql_nft_list = create_nft_list_table_sql(chain)?;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            conn.execute(&sql_nft_list, NO_PARAMS).map(|_| ())?;
            Ok(())
        })
        .await
    }

    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error> {
        let table_name = nft_list_table_name(chain);
        validate_table_name(&table_name)?;
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let nft_list_initialized = query_single_row(&conn, CHECK_TABLE_EXISTS_SQL, [table_name], string_from_row)?;
            Ok(nft_list_initialized.is_some())
        })
        .await
    }

    async fn get_nft_list(
        &self,
        chains: Vec<Chain>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
    ) -> MmResult<NftList, Self::Error> {
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let mut sql_builder = get_nft_list_builder_preimage(&conn, chains)?;
            let mut total_count_builder = sql_builder.clone();
            total_count_builder.count_all()?;
            let total: isize = total_count_builder
                .query_single_row(|row| row.get(0))?
                .or_mm_err(|| SqlError::QueryReturnedNoRows)?;
            let count_total = total.try_into().expect("count should not be failed");

            let (offset, limit) = if max {
                (0, count_total)
            } else {
                match page_number {
                    Some(page) => ((page.get() - 1) * limit, limit),
                    None => (0, limit),
                }
            };
            finalize_nft_list_sql_builder(&mut sql_builder, offset, limit)?;
            let nfts = sql_builder.query(nft_from_row)?;
            let result = NftList {
                nfts,
                skipped: offset,
                total: count_total,
            };
            Ok(result)
        })
        .await
    }

    async fn add_nfts_to_list<I>(&self, chain: &Chain, nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = Nft> + Send + 'static,
        I::IntoIter: Send,
    {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;

            for nft in nfts {
                let nft_json = json::to_string(&nft).expect("serialization should not fail");
                let params = [
                    Some(nft.chain.to_string()),
                    Some(nft.token_address),
                    Some(nft.token_id.to_string()),
                    Some(nft.amount.to_string()),
                    Some(nft.owner_of),
                    Some(nft.token_hash),
                    Some(nft.block_number_minted.to_string()),
                    Some(nft.block_number.to_string()),
                    nft.contract_type.map(|ct| ct.to_string()),
                    nft.name,
                    nft.symbol,
                    nft.token_uri,
                    nft.metadata,
                    nft.last_token_uri_sync,
                    nft.last_metadata_sync,
                    nft.minter_address,
                    nft.possible_spam.map(i32::from).map(|v| v.to_string()),
                    Some(nft_json),
                ];
                sql_transaction.execute(&insert_nft_in_list_sql(&chain)?, &params)?;
            }
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_nft(
        &self,
        _chain: &Chain,
        _token_address: String,
        _token_id: BigDecimal,
    ) -> MmResult<(), Self::Error> {
        todo!()
    }

    async fn remove_nft_from_list(
        &self,
        _chain: &Chain,
        _token_address: String,
        _token_id: BigDecimal,
    ) -> MmResult<(), Self::Error> {
        todo!()
    }
}

#[async_trait]
impl NftTxHistoryStorageOps for SqliteNftStorage {
    type Error = SqlError;

    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        let selfi = self.clone();
        let sql_tx_history = create_tx_history_table_sql(chain)?;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            conn.execute(&sql_tx_history, NO_PARAMS).map(|_| ())?;
            Ok(())
        })
        .await
    }

    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error> {
        let table_name = nft_tx_history_table_name(chain);
        validate_table_name(&table_name)?;
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let nft_list_initialized = query_single_row(&conn, CHECK_TABLE_EXISTS_SQL, [table_name], string_from_row)?;
            Ok(nft_list_initialized.is_some())
        })
        .await
    }

    async fn get_tx_history(
        &self,
        chains: Vec<Chain>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
        filters: Option<NftTxHistoryFilters>,
    ) -> MmResult<NftsTransferHistoryList, Self::Error> {
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            // todo get_nft_tx_builder_preimage complete filters
            let mut sql_builder = get_nft_tx_builder_preimage(&conn, chains, filters)?;
            let mut total_count_builder = sql_builder.clone();
            total_count_builder.count_all()?;
            let total: isize = total_count_builder
                .query_single_row(|row| row.get(0))?
                .or_mm_err(|| SqlError::QueryReturnedNoRows)?;
            let count_total = total.try_into().expect("count should not be failed");
            let (offset, limit) = if max {
                (0, count_total)
            } else {
                match page_number {
                    Some(page) => ((page.get() - 1) * limit, limit),
                    None => (0, limit),
                }
            };
            finalize_nft_history_sql_builder(&mut sql_builder, offset, limit)?;
            let txs = sql_builder.query(tx_history_from_row)?;
            let result = NftsTransferHistoryList {
                transfer_history: txs,
                skipped: offset,
                total: count_total,
            };
            Ok(result)
        })
        .await
    }

    async fn add_txs_to_history<I>(&self, chain: &Chain, txs: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = NftTransferHistory> + Send + 'static,
        I::IntoIter: Send,
    {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;

            for tx in txs {
                let tx_json = json::to_string(&tx).expect("serialization should not fail");
                let params = [
                    Some(tx.transaction_hash),
                    Some(tx.chain.to_string()),
                    Some(tx.block_number.to_string()),
                    Some(tx.block_timestamp),
                    Some(tx.block_hash),
                    Some(tx.transaction_index.to_string()),
                    Some(tx.log_index.to_string()),
                    Some(tx.value.to_string()),
                    Some(tx.contract_type.to_string()),
                    Some(tx.transaction_type.to_string()),
                    Some(tx.token_address),
                    Some(tx.token_id.to_string()),
                    Some(tx.from_address),
                    Some(tx.to_address),
                    Some(tx.amount.to_string()),
                    Some(tx.verified.to_string()),
                    tx.operator,
                    tx.possible_spam.map(i32::from).map(|v| v.to_string()),
                    Some(tx_json),
                ];
                sql_transaction.execute(&insert_tx_in_history_sql(&chain)?, &params)?;
            }
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_latest_block(&self, _chain: &Chain) -> MmResult<u64, Self::Error> { todo!() }
}
