mod kind;
pub mod mime;

pub use kind::HandlerKind;

use std::path::Path;

pub fn resolve_handler(path: &Path) -> Option<HandlerKind> {
    crate::orchestrator::registry::DriverRegistry::detect_builtin(path)
}

pub fn extensions_for(kind: HandlerKind) -> Vec<&'static str> {
    crate::orchestrator::extensions_for(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn md_is_markdown() {
        let mut f = NamedTempFile::with_suffix(".md").unwrap();
        write!(f, "# hi").unwrap();
        assert_eq!(resolve_handler(f.path()), Some(HandlerKind::Markdown));
    }

    #[test]
    fn py_is_code() {
        let mut f = NamedTempFile::with_suffix(".py").unwrap();
        write!(f, "print(1)").unwrap();
        assert_eq!(resolve_handler(f.path()), Some(HandlerKind::Code));
    }

    #[test]
    fn extensionless_gif_is_image() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"GIF89a\x01\x00\x01\x00\x00\xff\xff\xff,")
            .unwrap();
        assert_eq!(resolve_handler(f.path()), Some(HandlerKind::Image));
    }
}
