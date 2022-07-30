use astroport::{asset::PairInfo, router::SwapOperation};
use cosmwasm_std::{Addr};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the main control config of pair.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the type of pair info available in [`PairInfo`]
    pub pair_info: PairInfo,
    /// the router contract address
    pub router_addr: Addr,
    /// swap operations
    pub operations: Vec<SwapOperation>
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");