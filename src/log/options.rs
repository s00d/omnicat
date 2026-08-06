//! CLI options for `omnicat log`.

#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    pub json: bool,
    pub follow: bool,
    pub errors: bool,
    pub warnings: bool,
    pub level: Option<String>,
    pub stats: bool,
    pub timeline: bool,
    pub rate: bool,
    pub rate_errors: bool,
    pub top: Option<String>,
    pub top_limit: usize,
    pub slow: bool,
    pub slow_limit: usize,
    pub http: bool,
    pub status: Option<u16>,
    pub method: Option<String>,
    pub request: Option<String>,
    pub trace: Option<String>,
    pub around: Option<String>,
    pub context: Option<usize>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub where_clause: Option<String>,
    pub query: Option<String>,
    pub tail: Option<usize>,
    pub head: Option<usize>,
    pub progress: bool,
    pub allow_unsafe: bool,
    pub all: bool,
}

impl LogOptions {
    pub fn wants_aggregate(&self) -> bool {
        self.stats
            || self.timeline
            || self.rate
            || self.top.is_some()
            || self.slow
            || self.http
            || self.query.is_some()
    }

    pub fn wants_context(&self) -> bool {
        self.around.is_some()
    }

    pub fn is_live(&self) -> bool {
        self.follow
    }
}
