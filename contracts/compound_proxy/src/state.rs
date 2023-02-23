use cw_storage_plus::{Item};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum::router::Router;

/// This structure describes the main control config of pair.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// The router
    pub router: Router,
}

/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");
