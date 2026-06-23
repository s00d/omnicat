use std::io;

pub fn should_render(path: &str) -> bool {
    if !is_terminal::IsTerminal::is_terminal(&io::stdout()) {
        return false;
    }

    if path.starts_with('-') {
        return false;
    }

    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() || meta.is_dir(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_flag_as_file() {
        assert!(!should_render("-n"));
    }
}
