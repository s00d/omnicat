use std::path::Path;

use crate::config::{HandlerConfig, OmnicatConfig};
use crate::detect::mime::mime_matches;
use crate::detect::HandlerKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedHandler {
    Builtin(HandlerKind),
    Custom(String),
}

pub fn handler_kind_from_name(name: &str) -> Option<HandlerKind> {
    HandlerKind::all()
        .iter()
        .copied()
        .find(|k| k.name() == name)
}

pub fn is_builtin_handler_name(name: &str) -> bool {
    handler_kind_from_name(name).is_some()
}

pub fn detect_custom(path: &Path, config: &OmnicatConfig) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| !e.is_empty());

    for (name, handler) in &config.handlers {
        if is_builtin_handler_name(name) {
            continue;
        }
        if handler_matches_path(handler, path, ext.as_deref()) {
            return Some(name.clone());
        }
    }
    None
}

pub fn handler_matches_path(handler: &HandlerConfig, path: &Path, ext: Option<&str>) -> bool {
    if let Some(ext) = ext {
        if handler
            .extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(ext))
        {
            return true;
        }
    }
    if !handler.mime.is_empty() {
        if let Ok(bytes) = std::fs::read(path) {
            if let Some(guessed) = infer::get(&bytes) {
                let mime = guessed.mime_type();
                for pattern in &handler.mime {
                    if mime_matches(pattern, mime) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[allow(clippy::needless_lifetimes)]
pub fn handler_config_for_builtin<'a>(
    kind: HandlerKind,
    config: &'a OmnicatConfig,
) -> Option<&'a HandlerConfig> {
    config.handlers.get(kind.name())
}

pub fn handler_config_for_custom<'a>(
    name: &str,
    config: &'a OmnicatConfig,
) -> Option<&'a HandlerConfig> {
    config.handlers.get(name)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;
    use crate::detect::HandlerKind;
    use crate::orchestrator::PreviewOrchestrator;

    fn config_with_custom_notebook() -> OmnicatConfig {
        let mut handlers = HashMap::new();
        handlers.insert(
            "jupytext_notebook".into(),
            HandlerConfig {
                extensions: vec!["ipynb-custom".into()],
                commands: vec!["jupytext {file}".into()],
                hint: Some("brew install jupytext".into()),
                ..Default::default()
            },
        );
        OmnicatConfig {
            handlers,
            ..OmnicatConfig::default()
        }
    }

    #[test]
    fn custom_handler_matches_extension() {
        let mut f = NamedTempFile::with_suffix(".ipynb-custom").unwrap();
        write!(f, "{{}}").unwrap();
        let config = config_with_custom_notebook();
        let name = detect_custom(f.path(), &config).unwrap();
        assert_eq!(name, "jupytext_notebook");
    }

    #[test]
    fn builtin_takes_precedence_over_custom() {
        let mut f = NamedTempFile::with_suffix(".md").unwrap();
        write!(f, "# hi").unwrap();
        let config = config_with_custom_notebook();
        let resolved = PreviewOrchestrator::resolve(f.path(), &config).unwrap();
        assert_eq!(resolved, ResolvedHandler::Builtin(HandlerKind::Markdown));
    }

    #[test]
    fn handler_matches_path_by_extension() {
        let handler = HandlerConfig {
            extensions: vec!["xyz".into()],
            ..Default::default()
        };
        let path = std::path::Path::new("/tmp/file.xyz");
        assert!(handler_matches_path(&handler, path, Some("xyz")));
        assert!(!handler_matches_path(&handler, path, Some("md")));
    }
}
