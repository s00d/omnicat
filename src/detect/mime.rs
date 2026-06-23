use crate::detect::kind::HandlerKind;

pub const MIME_PATTERNS: &[(&str, HandlerKind)] = &[
    ("text/markdown", HandlerKind::Markdown),
    ("image/", HandlerKind::Image),
    ("application/pdf", HandlerKind::Pdf),
];

pub fn mime_matches(pattern: &str, mime: &str) -> bool {
    if pattern.ends_with('/') {
        mime.starts_with(pattern)
    } else if pattern.contains('*') {
        let prefix = pattern.trim_end_matches('*');
        mime.starts_with(prefix)
    } else {
        mime == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_wildcard() {
        assert!(mime_matches("image/", "image/gif"));
    }

    #[test]
    fn exact_match() {
        assert!(mime_matches("application/pdf", "application/pdf"));
    }
}
