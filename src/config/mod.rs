mod resolve;
mod schema;

pub use resolve::{load_config, resolved_config_path};
pub use schema::{
    AppConfig, BehaviorSettings, HandlerConfig, OmnicatConfig, PaginateDisplay,
};
