use crate::nft::nft_structs::{Chain, ConvertChain, Nft, NftList, NftTransferHistory};
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
    chain TEXT NOT NULL,
    token_address VARCHAR(256) NOT NULL,
    token_id VARCHAR(256) NOT NULL,
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
    PRIMARY KEY (token_address, token_id)
        );",
        table_name
    );
    Ok(sql)
}

#[allow(dead_code)]
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
    operator TEXT
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
    let mut union_sql_strings = Vec::new();
    for chain in chains.iter() {
        let table_name = nft_list_table_name(chain);
        validate_table_name(&table_name)?;
        let sql_builder = nft_list_table_builder_preimage(conn, table_name.as_str())?;
        let sql_string = sql_builder.sql()?.trim_end_matches(';').to_string();
        union_sql_strings.push(sql_string);
    }
    let union_sql = union_sql_strings.join(" UNION ALL ");
    let mut final_sql_builder = SqlQuery::select_from_union_alias(conn, union_sql.as_str(), "nft_list")?;
    final_sql_builder.order_desc("nft_list.block_number")?;
    Ok(final_sql_builder)
}

fn nft_list_table_builder_preimage<'a>(conn: &'a Connection, table_name: &'a str) -> MmResult<SqlQuery<'a>, SqlError> {
    let sql_builder = SqlQuery::select_from(conn, table_name)?;
    Ok(sql_builder)
}

fn finalize_get_nft_list_sql_builder(
    sql_builder: &mut SqlQuery,
    offset: usize,
    limit: usize,
) -> MmResult<(), SqlError> {
    sql_builder.offset(offset).limit(limit);
    Ok(())
}

fn nft_from_row(row: &Row<'_>) -> Result<Nft, SqlError> {
    let json_string: String = row.get(0)?;
    json::from_str(&json_string).map_err(|e| SqlError::FromSqlConversionFailure(0, Type::Text, Box::new(e)))
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
            finalize_get_nft_list_sql_builder(&mut sql_builder, offset, limit)?;
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

    async fn add_nfts_to_list<I>(&self, _chain: &Chain, _nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = Nft> + Send + 'static,
        I::IntoIter: Send,
    {
        let selfi = self.clone();
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let _sql_transaction = conn.transaction()?;

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

    async fn get_tx_history(&self, _ctx: &MmArc, _chain: &Chain) -> MmResult<Vec<NftTransferHistory>, Self::Error> {
        todo!()
    }

    async fn add_txs_to_history<I>(&self, _chain: &Chain, _nfts: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = NftTransferHistory> + Send + 'static,
        I::IntoIter: Send,
    {
        todo!()
    }
}
