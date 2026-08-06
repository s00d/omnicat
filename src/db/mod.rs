//! Database backup / persistence file inspector (`omnicat db`).

pub mod commands;
pub mod detect;
pub mod dispatch;
pub mod export;
pub mod mongo;
pub mod mysql;
pub mod options;
pub mod postgres;
pub mod query;
pub mod redis;
pub mod report;
pub mod sqlite;

pub use commands::run_db;
