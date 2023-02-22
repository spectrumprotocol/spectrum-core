use cosmwasm_std::{Addr};
use cw_storage_plus::{Item, Map};
use kujira::denom::Denom;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum::router::Router;

/// This structure stores the main parameter for the fees collector contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Address that's allowed to update config
    pub owner: Addr,
    /// Address that's allowed to update bridge asset
    pub operator: Addr,
    /// The factory contract address
    pub router: Router,
    /// The list of address and weight to receive fees
    pub target_list: Vec<(Addr, u64)>,
    /// The stablecoin token address
    pub stablecoin: Denom,
}

/// Stores the contract configuration at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores bridge tokens used to swap fee tokens to stablecoin
pub const BRIDGES: Map<String, Denom> = Map::new("bridges");
