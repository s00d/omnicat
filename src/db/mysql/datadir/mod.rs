//! MySQL datadir read-only inspector.

use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::db::report::MysqlDatadirOverview;

const WARN_RUNNING: &str =
    "Raw data directory may be inconsistent while mysqld is running; table data is not read.";

pub fn inspect_datadir(path: &Path) -> Result<MysqlDatadirOverview> {
    let mut ibdata_files = Vec::new();
    let mut tablespaces = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_ascii_lowercase();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if lower.starts_with("ibdata") || lower.starts_with("#innodb_redo") {
            ibdata_files.push((name, size));
        } else if lower.ends_with(".ibd") {
            tablespaces.push((name, size));
        }
    }
    ibdata_files.sort_by_key(|(_, sz)| std::cmp::Reverse(*sz));
    tablespaces.sort_by_key(|(_, sz)| std::cmp::Reverse(*sz));
    Ok(MysqlDatadirOverview {
        path: path.display().to_string(),
        ibdata_files,
        tablespaces,
        warnings: vec![WARN_RUNNING.into()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_ibd_and_ibdata() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ibdata1"), vec![0u8; 100]).unwrap();
        std::fs::write(dir.path().join("users.ibd"), vec![0u8; 50]).unwrap();
        let o = inspect_datadir(dir.path()).unwrap();
        assert_eq!(o.ibdata_files.len(), 1);
        assert_eq!(o.tablespaces.len(), 1);
        assert!(!o.warnings.is_empty());
    }
}
