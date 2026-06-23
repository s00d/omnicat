use syntect::highlighting::{Theme, ThemeSet};

pub fn resolve_theme<'a>(ts: &'a ThemeSet, name: &str) -> &'a Theme {
    const FALLBACKS: &[&str] = &["base16-ocean.dark", "Solarized (dark)", "InspiredGitHub"];
    if let Some(theme) = ts.themes.get(name) {
        return theme;
    }
    for fallback in FALLBACKS {
        if let Some(theme) = ts.themes.get(*fallback) {
            return theme;
        }
    }
    ts.themes.values().next().expect("theme set is non-empty")
}
