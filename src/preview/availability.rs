pub fn gui_available() -> bool {
    if std::env::var("OMNICAT_NO_GUI").ok().as_deref() == Some("1") {
        return false;
    }

    #[cfg(target_os = "linux")]
    {
        std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_ok()
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        true
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_gui_env_var() {
        std::env::set_var("OMNICAT_NO_GUI", "1");
        assert!(!gui_available());
        std::env::remove_var("OMNICAT_NO_GUI");
    }
}
