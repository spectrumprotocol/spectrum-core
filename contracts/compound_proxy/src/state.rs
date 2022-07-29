use astroport::asset::{PairInfo};
use cosmwasm_std::{Addr};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of pair.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the pair info
    pub pair_info: PairInfo,
    /// the swap commission
    pub commission_bps: u64,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores pair proxy for the given reward
pub const PAIR_PROXY: Map<String, Addr> = Map::new("pair_proxy");