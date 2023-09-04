use crate::nft::eth_addr_to_hex;
use crate::nft::nft_structs::{Chain, ContractType, ConvertChain, Nft, NftCommon, NftList, NftListFilters,
                              NftTokenAddrId, NftTransferCommon, NftTransferHistory, NftTransferHistoryFilters,
                              NftsTransferHistoryList, TransferMeta, UriMeta};
use crate::nft::storage::{get_offset_limit, CreateNftStorageError, NftDetailsJson, NftListStorageOps, NftStorageError,
                          NftTransferHistoryStorageOps, RemoveNftResult, TransferDetailsJson};
use async_trait::async_trait;
use common::async_blocking;
use db_common::sql_build::{SqlCondition, SqlQuery};
use db_common::sqlite::rusqlite::types::{FromSqlError, Type};
use db_common::sqlite::rusqlite::{Connection, Error as SqlError, Row, Statement};
use db_common::sqlite::sql_builder::SqlBuilder;
use db_common::sqlite::{query_single_row, string_from_row, validate_table_name, CHECK_TABLE_EXISTS_SQL};
use ethereum_types::Address;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::map_to_mm::MapToMmResult;
use mm2_err_handle::mm_error::{MmError, MmResult};
use mm2_number::BigDecimal;
use serde_json::Value as Json;
use serde_json::{self as json};
use std::convert::TryInto;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

fn nft_list_table_name(chain: &Chain) -> String { chain.to_ticker() + "_nft_list" }

fn nft_transfer_history_table_name(chain: &Chain) -> String { chain.to_ticker() + "_nft_transfer_history" }

fn scanned_nft_blocks_table_name() -> String { "scanned_nft_blocks".to_string() }

fn create_nft_list_table_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_list_table_name(chain);
    validate_table_name(&table_name)?;
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (
    token_address VARCHAR(256) NOT NULL,
    token_id VARCHAR(256) NOT NULL,
    chain TEXT NOT NULL,
    amount VARCHAR(256) NOT NULL,
    block_number INTEGER NOT NULL,
    contract_type TEXT NOT NULL,
    possible_spam INTEGER DEFAULT 0 NOT NULL,
    possible_phishing INTEGER DEFAULT 0 NOT NULL,
    collection_name TEXT,
    symbol TEXT,
    token_uri TEXT,
    token_domain TEXT,
    metadata TEXT,
    last_token_uri_sync TEXT,
    last_metadata_sync TEXT,
    raw_image_url TEXT,
    image_url TEXT,
    image_domain TEXT,
    token_name TEXT,
    description TEXT,
    attributes TEXT,
    animation_url TEXT,
    animation_domain TEXT,
    external_url TEXT,
    external_domain TEXT,
    image_details TEXT,
    details_json TEXT,
    PRIMARY KEY (token_address, token_id)
        );",
        table_name
    );
    Ok(sql)
}

fn create_transfer_history_table_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_transfer_history_table_name(chain);
    validate_table_name(&table_name)?;
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (
    transaction_hash VARCHAR(256) NOT NULL,
    log_index INTEGER NOT NULL,
    chain TEXT NOT NULL,
    block_number INTEGER NOT NULL,
    block_timestamp INTEGER NOT NULL,
    contract_type TEXT NOT NULL,
    token_address VARCHAR(256) NOT NULL,
    token_id VARCHAR(256) NOT NULL,
    status TEXT NOT NULL,
    amount VARCHAR(256) NOT NULL,
    possible_spam INTEGER DEFAULT 0 NOT NULL,
    possible_phishing INTEGER DEFAULT 0 NOT NULL,
    token_uri TEXT,
    token_domain TEXT,
    collection_name TEXT,
    image_url TEXT,
    image_domain TEXT,
    token_name TEXT,
    details_json TEXT,
    PRIMARY KEY (transaction_hash, log_index)
        );",
        table_name
    );
    Ok(sql)
}

fn create_scanned_nft_blocks_sql() -> MmResult<String, SqlError> {
    let table_name = scanned_nft_blocks_table_name();
    validate_table_name(&table_name)?;
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (
    chain TEXT PRIMARY KEY,
    last_scanned_block INTEGER DEFAULT 0
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

fn get_nft_list_builder_preimage(
    chains: Vec<Chain>,
    filters: Option<NftListFilters>,
) -> MmResult<SqlBuilder, SqlError> {
    let union_sql_strings = chains
        .iter()
        .map(|chain| {
            let table_name = nft_list_table_name(chain);
            validate_table_name(&table_name)?;
            let sql_builder = nft_list_builder_preimage(table_name.as_str(), filters)?;
            let sql_string = sql_builder
                .sql()
                .map_err(|e| SqlError::ToSqlConversionFailure(e.into()))?
                .trim_end_matches(';')
                .to_string();
            Ok(sql_string)
        })
        .collect::<MmResult<Vec<_>, SqlError>>()?;
    let union_alias_sql = format!("({}) AS nft_list", union_sql_strings.join(" UNION ALL "));
    let mut final_sql_builder = SqlBuilder::select_from(union_alias_sql);
    final_sql_builder.order_desc("nft_list.block_number");
    drop_mutability!(final_sql_builder);
    Ok(final_sql_builder)
}

fn nft_list_builder_preimage(table_name: &str, filters: Option<NftListFilters>) -> Result<SqlBuilder, SqlError> {
    let mut sql_builder = SqlBuilder::select_from(table_name);
    if let Some(filters) = filters {
        if filters.exclude_spam {
            sql_builder.and_where("possible_spam == 0");
        }
        if filters.exclude_phishing {
            sql_builder.and_where("possible_phishing == 0");
        }
    }
    drop_mutability!(sql_builder);
    Ok(sql_builder)
}

fn get_nft_transfer_builder_preimage(
    chains: Vec<Chain>,
    filters: Option<NftTransferHistoryFilters>,
) -> MmResult<SqlBuilder, SqlError> {
    let union_sql_strings = chains
        .into_iter()
        .map(|chain| {
            let table_name = nft_transfer_history_table_name(&chain);
            validate_table_name(&table_name)?;
            let sql_builder = nft_history_table_builder_preimage(table_name.as_str(), filters)?;
            let sql_string = sql_builder
                .sql()
                .map_err(|e| SqlError::ToSqlConversionFailure(e.into()))?
                .trim_end_matches(';')
                .to_string();
            Ok(sql_string)
        })
        .collect::<MmResult<Vec<_>, SqlError>>()?;
    let union_alias_sql = format!("({}) AS nft_history", union_sql_strings.join(" UNION ALL "));
    let mut final_sql_builder = SqlBuilder::select_from(union_alias_sql);
    final_sql_builder.order_desc("nft_history.block_timestamp");
    drop_mutability!(final_sql_builder);
    Ok(final_sql_builder)
}

fn nft_history_table_builder_preimage(
    table_name: &str,
    filters: Option<NftTransferHistoryFilters>,
) -> Result<SqlBuilder, SqlError> {
    let mut sql_builder = SqlBuilder::select_from(table_name);
    if let Some(filters) = filters {
        if filters.send && !filters.receive {
            sql_builder.and_where_eq("status", "'Send'");
        } else if filters.receive && !filters.send {
            sql_builder.and_where_eq("status", "'Receive'");
        }
        if let Some(date) = filters.from_date {
            sql_builder.and_where(format!("block_timestamp >= {}", date));
        }
        if let Some(date) = filters.to_date {
            sql_builder.and_where(format!("block_timestamp <= {}", date));
        }
        if filters.exclude_spam {
            sql_builder.and_where("possible_spam == 0");
        }
        if filters.exclude_phishing {
            sql_builder.and_where("possible_phishing == 0");
        }
    }
    drop_mutability!(sql_builder);
    Ok(sql_builder)
}

fn finalize_sql_builder(mut sql_builder: SqlBuilder, offset: usize, limit: usize) -> MmResult<String, SqlError> {
    let sql = sql_builder
        .field("*")
        .offset(offset)
        .limit(limit)
        .sql()
        .map_err(|e| SqlError::ToSqlConversionFailure(e.into()))?;
    Ok(sql)
}

fn get_and_parse<T: FromStr>(row: &Row<'_>, column: &str) -> Result<T, SqlError> {
    let value_str: String = row.get(column)?;
    value_str.parse().map_err(|_| SqlError::from(FromSqlError::InvalidType))
}

fn nft_from_row(row: &Row<'_>) -> Result<Nft, SqlError> {
    let token_address = get_and_parse(row, "token_address")?;
    let token_id = get_and_parse(row, "token_id")?;
    let chain = get_and_parse(row, "chain")?;
    let amount = get_and_parse(row, "amount")?;
    let block_number: u64 = row.get("block_number")?;
    let contract_type = get_and_parse(row, "contract_type")?;
    let possible_spam: i32 = row.get("possible_spam")?;
    let possible_phishing: i32 = row.get("possible_phishing")?;
    let collection_name: Option<String> = row.get("collection_name")?;
    let symbol: Option<String> = row.get("symbol")?;
    let token_uri: Option<String> = row.get("token_uri")?;
    let token_domain: Option<String> = row.get("token_domain")?;
    let metadata: Option<String> = row.get("metadata")?;
    let last_token_uri_sync: Option<String> = row.get("last_token_uri_sync")?;
    let last_metadata_sync: Option<String> = row.get("last_metadata_sync")?;
    let raw_image_url: Option<String> = row.get("raw_image_url")?;
    let image_url: Option<String> = row.get("image_url")?;
    let image_domain: Option<String> = row.get("image_domain")?;
    let token_name: Option<String> = row.get("token_name")?;
    let description: Option<String> = row.get("description")?;
    let attributes_str: Option<String> = row.get("attributes")?;
    let attributes: Option<Json> = attributes_str
        .as_deref()
        .map(json::from_str)
        .transpose()
        .map_err(|e| SqlError::FromSqlConversionFailure(21, Type::Text, Box::new(e)))?;
    let animation_url: Option<String> = row.get("animation_url")?;
    let animation_domain: Option<String> = row.get("animation_domain")?;
    let external_url: Option<String> = row.get("external_url")?;
    let external_domain: Option<String> = row.get("external_domain")?;
    let image_details_str: Option<String> = row.get("image_details")?;
    let image_details: Option<Json> = image_details_str
        .as_deref()
        .map(json::from_str)
        .transpose()
        .map_err(|e| SqlError::FromSqlConversionFailure(26, Type::Text, Box::new(e)))?;
    let details_json: String = row.get("details_json")?;
    let nft_details: NftDetailsJson =
        json::from_str(&details_json).map_err(|e| SqlError::FromSqlConversionFailure(27, Type::Text, Box::new(e)))?;

    let uri_meta = UriMeta {
        raw_image_url,
        image_url,
        image_domain,
        token_name,
        description,
        attributes,
        animation_url,
        animation_domain,
        external_url,
        external_domain,
        image_details,
    };

    let common = NftCommon {
        token_address,
        token_id,
        amount,
        owner_of: nft_details.owner_of,
        token_hash: nft_details.token_hash,
        collection_name,
        symbol,
        token_uri,
        token_domain,
        metadata,
        last_token_uri_sync,
        last_metadata_sync,
        minter_address: nft_details.minter_address,
        possible_spam: possible_spam != 0,
    };
    let nft = Nft {
        common,
        chain,
        block_number_minted: nft_details.block_number_minted,
        block_number,
        contract_type,
        possible_phishing: possible_phishing != 0,
        uri_meta,
    };
    Ok(nft)
}

fn transfer_history_from_row(row: &Row<'_>) -> Result<NftTransferHistory, SqlError> {
    let transaction_hash: String = row.get("transaction_hash")?;
    let log_index: u32 = row.get("log_index")?;
    let chain: Chain = get_and_parse(row, "chain")?;
    let block_number: u64 = row.get("block_number")?;
    let block_timestamp: u64 = row.get("block_timestamp")?;
    let contract_type: ContractType = get_and_parse(row, "contract_type")?;
    let token_address: Address = get_and_parse(row, "token_address")?;
    let token_id: BigDecimal = get_and_parse(row, "token_id")?;
    let status = get_and_parse(row, "status")?;
    let amount: BigDecimal = get_and_parse(row, "amount")?;
    let token_uri: Option<String> = row.get("token_uri")?;
    let token_domain: Option<String> = row.get("token_domain")?;
    let collection_name: Option<String> = row.get("collection_name")?;
    let image_url: Option<String> = row.get("image_url")?;
    let image_domain: Option<String> = row.get("image_domain")?;
    let token_name: Option<String> = row.get("token_name")?;
    let possible_spam: i32 = row.get("possible_spam")?;
    let possible_phishing: i32 = row.get("possible_phishing")?;
    let details_json: String = row.get("details_json")?;
    let details: TransferDetailsJson =
        json::from_str(&details_json).map_err(|e| SqlError::FromSqlConversionFailure(19, Type::Text, Box::new(e)))?;

    let common = NftTransferCommon {
        block_hash: details.block_hash,
        transaction_hash,
        transaction_index: details.transaction_index,
        log_index,
        value: details.value,
        transaction_type: details.transaction_type,
        token_address,
        token_id,
        from_address: details.from_address,
        to_address: details.to_address,
        amount,
        verified: details.verified,
        operator: details.operator,
        possible_spam: possible_spam != 0,
    };

    let transfer_history = NftTransferHistory {
        common,
        chain,
        block_number,
        block_timestamp,
        contract_type,
        token_uri,
        token_domain,
        collection_name,
        image_url,
        image_domain,
        token_name,
        status,
        possible_phishing: possible_phishing != 0,
    };

    Ok(transfer_history)
}

fn address_from_row(row: &Row<'_>) -> Result<Address, SqlError> {
    let address: String = row.get(0)?;
    address
        .parse()
        .map_err(|e| SqlError::FromSqlConversionFailure(0, Type::Text, Box::new(e)))
}

fn token_address_id_from_row(row: &Row<'_>) -> Result<NftTokenAddrId, SqlError> {
    let token_address: String = row.get("token_address")?;
    let token_id_str: String = row.get("token_id")?;
    let token_id = BigDecimal::from_str(&token_id_str).map_err(|_| SqlError::from(FromSqlError::InvalidType))?;
    Ok(NftTokenAddrId {
        token_address,
        token_id,
    })
}

fn insert_nft_in_list_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_list_table_name(chain);
    validate_table_name(&table_name)?;

    let sql = format!(
        "INSERT INTO {} (
            token_address, token_id, chain, amount, block_number, contract_type, possible_spam,
            possible_phishing, collection_name, symbol, token_uri, token_domain, metadata,
            last_token_uri_sync, last_metadata_sync, raw_image_url, image_url, image_domain,
            token_name, description, attributes, animation_url, animation_domain, external_url,
            external_domain, image_details, details_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
            ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27
        );",
        table_name
    );
    Ok(sql)
}

fn insert_transfer_in_history_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_transfer_history_table_name(chain);
    validate_table_name(&table_name)?;

    let sql = format!(
        "INSERT INTO {} (
            transaction_hash, log_index, chain, block_number, block_timestamp, contract_type,
            token_address, token_id, status, amount, token_uri, token_domain, collection_name, image_url, image_domain,
            token_name, possible_spam, possible_phishing, details_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19
        );",
        table_name
    );
    Ok(sql)
}

fn upsert_last_scanned_block_sql() -> MmResult<String, SqlError> {
    let table_name = scanned_nft_blocks_table_name();
    validate_table_name(&table_name)?;
    let sql = format!(
        "INSERT OR REPLACE INTO {} (chain, last_scanned_block) VALUES (?1, ?2);",
        table_name
    );
    Ok(sql)
}

fn refresh_nft_metadata_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_list_table_name(chain);

    validate_table_name(&table_name)?;
    let sql = format!(
        "UPDATE {} SET possible_spam = ?1, possible_phishing = ?2, collection_name = ?3, symbol = ?4, token_uri = ?5, token_domain = ?6, metadata = ?7, \
        last_token_uri_sync = ?8, last_metadata_sync = ?9, raw_image_url = ?10, image_url = ?11, image_domain = ?12, token_name = ?13, description = ?14, \
        attributes = ?15, animation_url = ?16, animation_domain = ?17, external_url = ?18, external_domain = ?19, image_details = ?20 WHERE token_address = ?21 AND token_id = ?22;",
        table_name
    );
    Ok(sql)
}

fn update_transfers_meta_by_token_addr_id_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_transfer_history_table_name(chain);

    validate_table_name(&table_name)?;
    let sql = format!(
        "UPDATE {} SET token_uri = ?1, token_domain = ?2, collection_name = ?3, image_url = ?4, image_domain = ?5, \
        token_name = ?6 WHERE token_address = ?7 AND token_id = ?8;",
        table_name
    );
    Ok(sql)
}

fn update_nft_amount_sql<F>(chain: &Chain, table_name_creator: F) -> MmResult<String, SqlError>
where
    F: FnOnce(&Chain) -> String,
{
    let table_name = table_name_creator(chain);

    validate_table_name(&table_name)?;
    let sql = format!(
        "UPDATE {} SET amount = ?1 WHERE token_address = ?2 AND token_id = ?3;",
        table_name
    );
    Ok(sql)
}

fn update_nft_amount_and_block_number_sql<F>(chain: &Chain, table_name_creator: F) -> MmResult<String, SqlError>
where
    F: FnOnce(&Chain) -> String,
{
    let table_name = table_name_creator(chain);

    validate_table_name(&table_name)?;
    let sql = format!(
        "UPDATE {} SET amount = ?1, block_number = ?2 WHERE token_address = ?3 AND token_id = ?4;",
        table_name
    );
    Ok(sql)
}

fn get_nft_metadata_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_list_table_name(chain);
    validate_table_name(&table_name)?;
    let sql = format!("SELECT * FROM {} WHERE token_address=?1 AND token_id=?2", table_name);
    Ok(sql)
}

fn select_last_block_number_sql<F>(chain: &Chain, table_name_creator: F) -> MmResult<String, SqlError>
where
    F: FnOnce(&Chain) -> String,
{
    let table_name = table_name_creator(chain);
    validate_table_name(&table_name)?;
    let sql = format!(
        "SELECT block_number FROM {} ORDER BY block_number DESC LIMIT 1",
        table_name
    );
    Ok(sql)
}

fn select_last_scanned_block_sql() -> MmResult<String, SqlError> {
    let table_name = scanned_nft_blocks_table_name();
    validate_table_name(&table_name)?;
    let sql = format!("SELECT last_scanned_block FROM {} WHERE chain=?1", table_name,);
    Ok(sql)
}

fn get_nft_amount_sql<F>(chain: &Chain, table_name_creator: F) -> MmResult<String, SqlError>
where
    F: FnOnce(&Chain) -> String,
{
    let table_name = table_name_creator(chain);
    validate_table_name(&table_name)?;
    let sql = format!(
        "SELECT amount FROM {} WHERE token_address=?1 AND token_id=?2",
        table_name
    );
    Ok(sql)
}

fn delete_nft_sql<F>(chain: &Chain, table_name_creator: F) -> Result<String, MmError<SqlError>>
where
    F: FnOnce(&Chain) -> String,
{
    let table_name = table_name_creator(chain);
    validate_table_name(&table_name)?;
    let sql = format!("DELETE FROM {} WHERE token_address=?1 AND token_id=?2", table_name);
    Ok(sql)
}

fn block_number_from_row(row: &Row<'_>) -> Result<i64, SqlError> { row.get::<_, i64>(0) }

fn nft_amount_from_row(row: &Row<'_>) -> Result<String, SqlError> { row.get(0) }

fn get_nfts_by_token_address_statement<'a, F>(
    conn: &'a Connection,
    chain: &'a Chain,
    table_name_creator: F,
) -> MmResult<Statement<'a>, SqlError>
where
    F: FnOnce(&Chain) -> String,
{
    let table_name = table_name_creator(chain);
    validate_table_name(table_name.as_str())?;
    let sql_query = format!("SELECT * FROM {} WHERE token_address = ?", table_name);
    let stmt = conn.prepare(&sql_query)?;
    Ok(stmt)
}

fn get_token_addresses_statement<'a, F>(
    conn: &'a Connection,
    chain: &'a Chain,
    table_name_creator: F,
) -> MmResult<Statement<'a>, SqlError>
where
    F: FnOnce(&Chain) -> String,
{
    let table_name = table_name_creator(chain);
    validate_table_name(table_name.as_str())?;
    let sql_query = format!("SELECT DISTINCT token_address FROM {}", table_name);
    let stmt = conn.prepare(&sql_query)?;
    Ok(stmt)
}

// fn get_transfers_from_block_builder<'a>(
//     conn: &'a Connection,
//     chain: &'a Chain,
//     from_block: u64,
// ) -> MmResult<SqlQuery<'a>, SqlError> {
//     let table_name = nft_transfer_history_table_name(chain);
//     validate_table_name(table_name.as_str())?;
//     let mut sql_builder = SqlQuery::select_from(conn, table_name.as_str())?;
//     sql_builder
//         .sql_builder()
//         .and_where(format!("block_number >= '{}'", from_block))
//         .order_asc("block_number")
//         .field("*");
//     drop_mutability!(sql_builder);
//     Ok(sql_builder)
// }

fn get_transfers_from_block_statement<'a>(conn: &'a Connection, chain: &'a Chain) -> MmResult<Statement<'a>, SqlError> {
    let table_name = nft_transfer_history_table_name(chain);
    validate_table_name(table_name.as_str())?;
    let sql_query = format!(
        "SELECT * FROM {} WHERE block_number >= ? ORDER BY block_number ASC",
        table_name
    );
    let stmt = conn.prepare(&sql_query)?;
    Ok(stmt)
}

fn get_transfers_by_token_addr_id_statement<'a>(
    conn: &'a Connection,
    chain: &'a Chain,
) -> MmResult<Statement<'a>, SqlError> {
    let table_name = nft_transfer_history_table_name(chain);
    validate_table_name(table_name.as_str())?;
    let sql_query = format!("SELECT * FROM {} WHERE token_address = ? AND token_id = ?", table_name);
    let stmt = conn.prepare(&sql_query)?;
    Ok(stmt)
}

fn get_transfers_with_empty_meta_builder<'a>(
    conn: &'a Connection,
    chain: &'a Chain,
) -> MmResult<SqlQuery<'a>, SqlError> {
    let table_name = nft_transfer_history_table_name(chain);
    validate_table_name(table_name.as_str())?;
    let mut sql_builder = SqlQuery::select_from(conn, table_name.as_str())?;
    sql_builder
        .sql_builder()
        .distinct()
        .field("token_address")
        .field("token_id")
        .and_where_is_null("token_uri")
        .and_where_is_null("collection_name")
        .and_where_is_null("image_url")
        .and_where_is_null("token_name");
    drop_mutability!(sql_builder);
    Ok(sql_builder)
}

fn get_transfer_by_tx_hash_and_log_index_sql(chain: &Chain) -> MmResult<String, SqlError> {
    let table_name = nft_transfer_history_table_name(chain);
    validate_table_name(&table_name)?;
    let sql = format!(
        "SELECT * FROM {} WHERE transaction_hash=?1 AND log_index = ?2",
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
            conn.execute(&sql_nft_list, []).map(|_| ())?;
            conn.execute(&create_scanned_nft_blocks_sql()?, []).map(|_| ())?;
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
            let scanned_nft_blocks_initialized = query_single_row(
                &conn,
                CHECK_TABLE_EXISTS_SQL,
                [scanned_nft_blocks_table_name()],
                string_from_row,
            )?;
            Ok(nft_list_initialized.is_some() && scanned_nft_blocks_initialized.is_some())
        })
        .await
    }

    async fn get_nft_list(
        &self,
        chains: Vec<Chain>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
        filters: Option<NftListFilters>,
    ) -> MmResult<NftList, Self::Error> {
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let sql_builder = get_nft_list_builder_preimage(chains, filters)?;
            let total_count_builder_sql = sql_builder
                .clone()
                .count("*")
                .sql()
                .map_err(|e| SqlError::ToSqlConversionFailure(e.into()))?;
            let total: isize = conn
                .prepare(&total_count_builder_sql)?
                .query_row([], |row| row.get(0))?;
            let count_total = total.try_into().expect("count should not be failed");

            let (offset, limit) = get_offset_limit(max, limit, page_number, count_total);
            let sql = finalize_sql_builder(sql_builder, offset, limit)?;
            let nfts = conn
                .prepare(&sql)?
                .query_map([], nft_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            let result = NftList {
                nfts,
                skipped: offset,
                total: count_total,
            };
            Ok(result)
        })
        .await
    }

    async fn add_nfts_to_list<I>(&self, chain: &Chain, nfts: I, last_scanned_block: u64) -> MmResult<(), Self::Error>
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
                let details_json = NftDetailsJson {
                    owner_of: nft.common.owner_of,
                    token_hash: nft.common.token_hash,
                    minter_address: nft.common.minter_address,
                    block_number_minted: nft.block_number_minted,
                };
                let details_json = json::to_string(&details_json).expect("serialization should not fail");
                let params = [
                    Some(eth_addr_to_hex(&nft.common.token_address)),
                    Some(nft.common.token_id.to_string()),
                    Some(nft.chain.to_string()),
                    Some(nft.common.amount.to_string()),
                    Some(nft.block_number.to_string()),
                    Some(nft.contract_type.to_string()),
                    Some(i32::from(nft.common.possible_spam).to_string()),
                    Some(i32::from(nft.possible_phishing).to_string()),
                    nft.common.collection_name,
                    nft.common.symbol,
                    nft.common.token_uri,
                    nft.common.token_domain,
                    nft.common.metadata,
                    nft.common.last_token_uri_sync,
                    nft.common.last_metadata_sync,
                    nft.uri_meta.raw_image_url,
                    nft.uri_meta.image_url,
                    nft.uri_meta.image_domain,
                    nft.uri_meta.token_name,
                    nft.uri_meta.description,
                    nft.uri_meta.attributes.map(|v| v.to_string()),
                    nft.uri_meta.animation_url,
                    nft.uri_meta.animation_domain,
                    nft.uri_meta.external_url,
                    nft.uri_meta.external_domain,
                    nft.uri_meta.image_details.map(|v| v.to_string()),
                    Some(details_json),
                ];
                sql_transaction.execute(&insert_nft_in_list_sql(&chain)?, params)?;
            }
            let scanned_block_params = [chain.to_ticker(), last_scanned_block.to_string()];
            sql_transaction.execute(&upsert_last_scanned_block_sql()?, scanned_block_params)?;
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_nft(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Option<Nft>, Self::Error> {
        let sql = get_nft_metadata_sql(chain)?;
        let params = [token_address, token_id.to_string()];
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            query_single_row(&conn, &sql, params, nft_from_row).map_to_mm(SqlError::from)
        })
        .await
    }

    async fn remove_nft_from_list(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
        scanned_block: u64,
    ) -> MmResult<RemoveNftResult, Self::Error> {
        let sql = delete_nft_sql(chain, nft_list_table_name)?;
        let params = [token_address, token_id.to_string()];
        let scanned_block_params = [chain.to_ticker(), scanned_block.to_string()];
        let selfi = self.clone();
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;
            let rows_num = sql_transaction.execute(&sql, params)?;

            let remove_nft_result = if rows_num > 0 {
                RemoveNftResult::NftRemoved
            } else {
                RemoveNftResult::NftDidNotExist
            };
            sql_transaction.execute(&upsert_last_scanned_block_sql()?, scanned_block_params)?;
            sql_transaction.commit()?;
            Ok(remove_nft_result)
        })
        .await
    }

    async fn get_nft_amount(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Option<String>, Self::Error> {
        let sql = get_nft_amount_sql(chain, nft_list_table_name)?;
        let params = [token_address, token_id.to_string()];
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            query_single_row(&conn, &sql, params, nft_amount_from_row).map_to_mm(SqlError::from)
        })
        .await
    }

    async fn refresh_nft_metadata(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error> {
        let sql = refresh_nft_metadata_sql(chain)?;
        let selfi = self.clone();
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;
            let params = [
                Some(i32::from(nft.common.possible_spam).to_string()),
                Some(i32::from(nft.possible_phishing).to_string()),
                nft.common.collection_name,
                nft.common.symbol,
                nft.common.token_uri,
                nft.common.token_domain,
                nft.common.metadata,
                nft.common.last_token_uri_sync,
                nft.common.last_metadata_sync,
                nft.uri_meta.raw_image_url,
                nft.uri_meta.image_url,
                nft.uri_meta.image_domain,
                nft.uri_meta.token_name,
                nft.uri_meta.description,
                nft.uri_meta.attributes.map(|v| v.to_string()),
                nft.uri_meta.animation_url,
                nft.uri_meta.animation_domain,
                nft.uri_meta.external_url,
                nft.uri_meta.external_domain,
                nft.uri_meta.image_details.map(|v| v.to_string()),
                Some(eth_addr_to_hex(&nft.common.token_address)),
                Some(nft.common.token_id.to_string()),
            ];
            sql_transaction.execute(&sql, params)?;
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_last_block_number(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let sql = select_last_block_number_sql(chain, nft_list_table_name)?;
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            query_single_row(&conn, &sql, [], block_number_from_row).map_to_mm(SqlError::from)
        })
        .await?
        .map(|b| b.try_into())
        .transpose()
        .map_to_mm(|e| SqlError::FromSqlConversionFailure(2, Type::Integer, Box::new(e)))
    }

    async fn get_last_scanned_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let sql = select_last_scanned_block_sql()?;
        let params = [chain.to_ticker()];
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            query_single_row(&conn, &sql, params, block_number_from_row).map_to_mm(SqlError::from)
        })
        .await?
        .map(|b| b.try_into())
        .transpose()
        .map_to_mm(|e| SqlError::FromSqlConversionFailure(2, Type::Integer, Box::new(e)))
    }

    async fn update_nft_amount(&self, chain: &Chain, nft: Nft, scanned_block: u64) -> MmResult<(), Self::Error> {
        let sql = update_nft_amount_sql(chain, nft_list_table_name)?;
        let scanned_block_params = [chain.to_ticker(), scanned_block.to_string()];
        let selfi = self.clone();
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;
            let params = [
                Some(nft.common.amount.to_string()),
                Some(eth_addr_to_hex(&nft.common.token_address)),
                Some(nft.common.token_id.to_string()),
            ];
            sql_transaction.execute(&sql, params)?;
            sql_transaction.execute(&upsert_last_scanned_block_sql()?, scanned_block_params)?;
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn update_nft_amount_and_block_number(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error> {
        let sql = update_nft_amount_and_block_number_sql(chain, nft_list_table_name)?;
        let scanned_block_params = [chain.to_ticker(), nft.block_number.to_string()];
        let selfi = self.clone();
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;
            let params = [
                Some(nft.common.amount.to_string()),
                Some(nft.block_number.to_string()),
                Some(eth_addr_to_hex(&nft.common.token_address)),
                Some(nft.common.token_id.to_string()),
            ];
            sql_transaction.execute(&sql, params)?;
            sql_transaction.execute(&upsert_last_scanned_block_sql()?, scanned_block_params)?;
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_nfts_by_token_address(&self, chain: &Chain, token_address: String) -> MmResult<Vec<Nft>, Self::Error> {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let mut stmt = get_nfts_by_token_address_statement(&conn, &chain, nft_list_table_name)?;
            let nfts = stmt
                .query_map([token_address], nft_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(nfts)
        })
        .await
    }

    async fn update_nft_spam_by_token_address(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error> {
        let selfi = self.clone();

        let table_name = nft_list_table_name(chain);
        validate_table_name(&table_name)?;
        let sql = format!("UPDATE {} SET possible_spam = ?1 WHERE token_address = ?2;", table_name);

        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;
            let params = [Some(i32::from(possible_spam).to_string()), Some(token_address.clone())];
            sql_transaction.execute(&sql, params)?;
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }
}

#[async_trait]
impl NftTransferHistoryStorageOps for SqliteNftStorage {
    type Error = SqlError;

    async fn init(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        let selfi = self.clone();
        let sql_transfer_history = create_transfer_history_table_sql(chain)?;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            conn.execute(&sql_transfer_history, []).map(|_| ())?;
            Ok(())
        })
        .await
    }

    async fn is_initialized(&self, chain: &Chain) -> MmResult<bool, Self::Error> {
        let table_name = nft_transfer_history_table_name(chain);
        validate_table_name(&table_name)?;
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let nft_list_initialized = query_single_row(&conn, CHECK_TABLE_EXISTS_SQL, [table_name], string_from_row)?;
            Ok(nft_list_initialized.is_some())
        })
        .await
    }

    async fn get_transfer_history(
        &self,
        chains: Vec<Chain>,
        max: bool,
        limit: usize,
        page_number: Option<NonZeroUsize>,
        filters: Option<NftTransferHistoryFilters>,
    ) -> MmResult<NftsTransferHistoryList, Self::Error> {
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let sql_builder = get_nft_transfer_builder_preimage(chains, filters)?;
            let total_count_builder_sql = sql_builder
                .clone()
                .count("*")
                .sql()
                .map_err(|e| SqlError::ToSqlConversionFailure(e.into()))?;
            let total: isize = conn
                .prepare(&total_count_builder_sql)?
                .query_row([], |row| row.get(0))?;
            let count_total = total.try_into().expect("count should not be failed");

            let (offset, limit) = get_offset_limit(max, limit, page_number, count_total);
            let sql = finalize_sql_builder(sql_builder, offset, limit)?;
            let transfers = conn
                .prepare(&sql)?
                .query_map([], transfer_history_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            let result = NftsTransferHistoryList {
                transfer_history: transfers,
                skipped: offset,
                total: count_total,
            };
            Ok(result)
        })
        .await
    }

    async fn add_transfers_to_history<I>(&self, chain: &Chain, transfers: I) -> MmResult<(), Self::Error>
    where
        I: IntoIterator<Item = NftTransferHistory> + Send + 'static,
        I::IntoIter: Send,
    {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;

            for transfer in transfers {
                let details_json = TransferDetailsJson {
                    block_hash: transfer.common.block_hash,
                    transaction_index: transfer.common.transaction_index,
                    value: transfer.common.value,
                    transaction_type: transfer.common.transaction_type,
                    verified: transfer.common.verified,
                    operator: transfer.common.operator,
                    from_address: transfer.common.from_address,
                    to_address: transfer.common.from_address,
                };
                let transfer_json = json::to_string(&details_json).expect("serialization should not fail");
                let params = [
                    Some(transfer.common.transaction_hash),
                    Some(transfer.common.log_index.to_string()),
                    Some(transfer.chain.to_string()),
                    Some(transfer.block_number.to_string()),
                    Some(transfer.block_timestamp.to_string()),
                    Some(transfer.contract_type.to_string()),
                    Some(eth_addr_to_hex(&transfer.common.token_address)),
                    Some(transfer.common.token_id.to_string()),
                    Some(transfer.status.to_string()),
                    Some(transfer.common.amount.to_string()),
                    transfer.token_uri,
                    transfer.token_domain,
                    transfer.collection_name,
                    transfer.image_url,
                    transfer.image_domain,
                    transfer.token_name,
                    Some(i32::from(transfer.common.possible_spam).to_string()),
                    Some(i32::from(transfer.possible_phishing).to_string()),
                    Some(transfer_json),
                ];
                sql_transaction.execute(&insert_transfer_in_history_sql(&chain)?, params)?;
            }
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_last_block_number(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let sql = select_last_block_number_sql(chain, nft_transfer_history_table_name)?;
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            query_single_row(&conn, &sql, [], block_number_from_row).map_to_mm(SqlError::from)
        })
        .await?
        .map(|b| b.try_into())
        .transpose()
        .map_to_mm(|e| SqlError::FromSqlConversionFailure(2, Type::Integer, Box::new(e)))
    }

    async fn get_transfers_from_block(
        &self,
        chain: &Chain,
        from_block: u64,
    ) -> MmResult<Vec<NftTransferHistory>, Self::Error> {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let mut stmt = get_transfers_from_block_statement(&conn, &chain)?;
            let transfers = stmt
                .query_map([from_block], transfer_history_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(transfers)
        })
        .await
    }

    async fn get_transfers_by_token_addr_id(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigDecimal,
    ) -> MmResult<Vec<NftTransferHistory>, Self::Error> {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let mut stmt = get_transfers_by_token_addr_id_statement(&conn, &chain)?;
            let transfers = stmt
                .query_map([token_address, token_id.to_string()], transfer_history_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(transfers)
        })
        .await
    }

    async fn get_transfer_by_tx_hash_and_log_index(
        &self,
        chain: &Chain,
        transaction_hash: String,
        log_index: u32,
    ) -> MmResult<Option<NftTransferHistory>, Self::Error> {
        let sql = get_transfer_by_tx_hash_and_log_index_sql(chain)?;
        let selfi = self.clone();
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            query_single_row(
                &conn,
                &sql,
                [transaction_hash, log_index.to_string()],
                transfer_history_from_row,
            )
            .map_to_mm(SqlError::from)
        })
        .await
    }

    async fn update_transfers_meta_by_token_addr_id(
        &self,
        chain: &Chain,
        transfer_meta: TransferMeta,
    ) -> MmResult<(), Self::Error> {
        let sql = update_transfers_meta_by_token_addr_id_sql(chain)?;
        let params = [
            transfer_meta.token_uri,
            transfer_meta.token_domain,
            transfer_meta.collection_name,
            transfer_meta.image_url,
            transfer_meta.image_domain,
            transfer_meta.token_name,
            Some(transfer_meta.token_address),
            Some(transfer_meta.token_id.to_string()),
        ];
        let selfi = self.clone();
        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;
            sql_transaction.execute(&sql, params)?;
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_transfers_with_empty_meta(&self, chain: &Chain) -> MmResult<Vec<NftTokenAddrId>, Self::Error> {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let sql_builder = get_transfers_with_empty_meta_builder(&conn, &chain)?;
            let token_addr_id_pair = sql_builder.query(token_address_id_from_row)?;
            Ok(token_addr_id_pair)
        })
        .await
    }

    async fn get_transfers_by_token_address(
        &self,
        chain: &Chain,
        token_address: String,
    ) -> MmResult<Vec<NftTransferHistory>, Self::Error> {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let mut stmt = get_nfts_by_token_address_statement(&conn, &chain, nft_transfer_history_table_name)?;
            let nfts = stmt
                .query_map([token_address], transfer_history_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(nfts)
        })
        .await
    }

    async fn update_transfer_spam_by_token_address(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error> {
        let selfi = self.clone();

        let table_name = nft_transfer_history_table_name(chain);
        validate_table_name(&table_name)?;
        let sql = format!("UPDATE {} SET possible_spam = ?1 WHERE token_address = ?2;", table_name);

        async_blocking(move || {
            let mut conn = selfi.0.lock().unwrap();
            let sql_transaction = conn.transaction()?;
            let params = [Some(i32::from(possible_spam).to_string()), Some(token_address.clone())];
            sql_transaction.execute(&sql, params)?;
            sql_transaction.commit()?;
            Ok(())
        })
        .await
    }

    async fn get_token_addresses(&self, chain: &Chain) -> MmResult<Vec<Address>, Self::Error> {
        let selfi = self.clone();
        let chain = *chain;
        async_blocking(move || {
            let conn = selfi.0.lock().unwrap();
            let mut stmt = get_token_addresses_statement(&conn, &chain, nft_transfer_history_table_name)?;
            let addresses = stmt.query_map([], address_from_row)?.collect::<Result<Vec<_>, _>>()?;
            Ok(addresses)
        })
        .await
    }
}
