//! Hidden helpers for criterion benches (`benches/perf.rs`).

pub use crate::config::OmnicatConfig;
pub use crate::drivers::sql_dump::{parse_dump_select, query_sql_dump};
pub use crate::io::{open_reader, LineIter};
pub use crate::sinks::progress::ProgressReporter;
