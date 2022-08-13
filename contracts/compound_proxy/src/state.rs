use astroport::asset::{PairInfo};
use cosmwasm_std::{Decimal};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum::adapters::pair::Pair;

/// ## Description
/// This structure describes the main control config of pair.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// the pair info
    pub pair_info: PairInfo,
    /// the swap commission
    pub commission_bps: u64,
    /// the slippage tolerance when providing liquidity
    pub slippage_tolerance: Decimal,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores pair proxy for the given reward
pub const PAIR_PROXY: Map<String, Pair> = Map::new("pair_proxy");
