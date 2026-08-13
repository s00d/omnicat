//! HTML → Markdown via [`htmd`], shared by email and ebook (mobi) drivers.

/// Convert an HTML fragment/document to GitHub-Flavored Markdown.
/// Falls back to a minimal tag strip if `htmd` fails.
pub fn html_to_markdown(html: &str) -> String {
    match htmd::convert(html) {
        Ok(md) => md,
        Err(_) => strip_tags(html),
    }
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_simple_html() {
        let md = html_to_markdown("<p>Hello <b>world</b></p>");
        assert!(md.to_lowercase().contains("hello"));
        assert!(md.to_lowercase().contains("world"));
    }
}
