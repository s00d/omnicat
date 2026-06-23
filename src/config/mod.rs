mod resolve;
mod schema;

pub use resolve::{load_config, resolved_config_path};
pub use schema::{
    AppConfig, BehaviorSettings, DisplayConfig, HandlerConfig, OmnicatConfig, PaginateDisplay,
};

// backward compat
pub fn load_display_config() -> anyhow::Result<OmnicatConfig> {
    load_config()
}
