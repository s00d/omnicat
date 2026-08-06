//! CLI options for `omnicat db`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DbOutputFormat {
    #[default]
    Table,
    Csv,
    Json,
    Jsonl,
}

#[derive(Debug, Clone, Default)]
pub struct DbOptions {
    pub json: bool,
    pub schema: bool,
    pub tables: bool,
    pub stats: bool,
    pub query: Option<String>,
    pub table: Option<String>,
    pub sample: Option<usize>,
    pub find: Option<String>,
    pub top: Option<String>,
    pub top_limit: usize,
    pub output: DbOutputFormat,
    pub extract: Option<String>,
    pub progress: bool,
    pub print_query: bool,
    pub all: bool,
}

impl DbOptions {
    pub fn wants_query(&self) -> bool {
        self.query.is_some()
    }

    pub fn is_inspect_mode(&self) -> bool {
        self.schema
            || self.tables
            || self.stats
            || self.query.is_some()
            || self.sample.is_some()
            || self.find.is_some()
            || self.top.is_some()
    }
}
