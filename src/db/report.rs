//! Structured reports for `omnicat db`.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub rows: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MysqlDumpStats {
    pub path: String,
    pub bytes_scanned: u64,
    pub tables: Vec<TableInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MysqlDumpOverview {
    pub path: String,
    pub tables: u64,
    pub inserts: u64,
    pub bytes_scanned: u64,
    pub largest: Vec<TableInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MysqlDatadirOverview {
    pub path: String,
    pub ibdata_files: Vec<(String, u64)>,
    pub tablespaces: Vec<(String, u64)>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedisRdbStats {
    pub path: String,
    pub version: Option<String>,
    pub keys: u64,
    pub types: BTreeMap<String, u64>,
    pub memory_estimate: u64,
    pub patterns: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedisAofStats {
    pub path: String,
    pub commands: u64,
    pub by_command: BTreeMap<String, u64>,
    pub first_line: Option<String>,
    pub last_line: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeySample {
    pub key: String,
    pub kind: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresDumpOverview {
    pub path: String,
    pub format: String,
    pub databases: Vec<String>,
    pub tables: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MongoDumpOverview {
    pub path: String,
    pub collections: u64,
    pub documents: u64,
    pub bytes_scanned: u64,
    pub largest: Vec<TableInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MongoDatadirOverview {
    pub path: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SqliteOverview {
    pub path: String,
    pub tables: Vec<TableInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DynamoDbExportOverview {
    pub path: String,
    pub format: String,
    pub tables: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ElasticsearchSnapshotOverview {
    pub path: String,
    pub indices: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryResultReport {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub rows_scanned: u64,
    pub rows_matched: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DbReport {
    OverviewMysqlDump(MysqlDumpOverview),
    StatsMysqlDump(MysqlDumpStats),
    OverviewPostgresDump(PostgresDumpOverview),
    OverviewMongoDump(MongoDumpOverview),
    StatsMongoDump(MongoDumpOverview),
    OverviewMongoDatadir(MongoDatadirOverview),
    OverviewSqlite(SqliteOverview),
    OverviewDynamoDb(DynamoDbExportOverview),
    OverviewElasticsearch(ElasticsearchSnapshotOverview),
    OverviewMysqlDatadir(MysqlDatadirOverview),
    Tables {
        tables: Vec<TableInfo>,
    },
    Schema {
        tables: Vec<TableSchema>,
    },
    RedisRdb(RedisRdbStats),
    RedisAof(RedisAofStats),
    Samples {
        items: Vec<KeySample>,
    },
    Find {
        matches: Vec<String>,
    },
    Top {
        field: String,
        items: Vec<(String, u64)>,
    },
    Query(QueryResultReport),
    Text {
        lines: Vec<String>,
    },
}
