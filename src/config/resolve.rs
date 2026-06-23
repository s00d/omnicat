use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::config::schema::OmnicatConfig;

pub fn load_config() -> Result<OmnicatConfig> {
    let path = resolved_config_path()?;
    if let Some(path) = path {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let cfg: OmnicatConfig = serde_yaml::from_str(&text)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        Ok(cfg)
    } else {
        Ok(OmnicatConfig::default())
    }
}

pub fn resolved_config_path() -> Result<Option<PathBuf>> {
    for env_key in ["OMNICAT_CONFIG", "SMARTCAT_CONFIG"] {
        if let Ok(path) = std::env::var(env_key) {
            let path = PathBuf::from(path);
            if path.is_file() {
                return Ok(Some(path));
            }
        }
    }

    for subdir in ["omnicat", "smartcat"] {
        if let Some(user) = user_config_path(subdir) {
            if user.is_file() {
                return Ok(Some(user));
            }
        }
    }

    Ok(None)
}

fn user_config_path(subdir: &str) -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join(format!("{subdir}/config.yaml")));
    }

    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(PathBuf::from(appdata).join(format!("{subdir}/config.yaml")));
        }
    }

    dirs_home().map(|home| home.join(format!(".config/{subdir}/config.yaml")))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omnicat_config_override() {
        let dir = tempfile::tempdir().unwrap();
        let cfg_path = dir.path().join("cfg.yaml");
        fs::write(
            &cfg_path,
            "terminal:\n  code:\n    line_numbers: false\n    theme: x\n    style: plain\n    tab_width: 2\n",
        )
        .unwrap();
        std::env::set_var("OMNICAT_CONFIG", cfg_path.to_string_lossy().to_string());
        let cfg = load_config().unwrap();
        assert!(!cfg.terminal.code.line_numbers);
        std::env::remove_var("OMNICAT_CONFIG");
    }
}
