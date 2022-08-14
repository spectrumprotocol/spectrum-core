use astroport::{asset::AssetInfo, common::OwnershipProposal};
use cosmwasm_std::{Addr, Decimal};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure stores the main parameter for the fees collector contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// Address that's allowed to update config
    pub owner: Addr,
    /// Address that's allowed to update bridge asset
    pub operator: Addr,
    /// The factory contract address
    pub factory_contract: Addr,
    /// The list of address and weight to receive fees
    pub target_list: Vec<(Addr, u64)>,
    /// The stablecoin token address
    pub stablecoin: AssetInfo,
    /// The max spread allowed when swapping fee tokens to stablecoin
    pub max_spread: Decimal,
}

/// ## Description
/// Stores the contract configuration at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// Stores bridge tokens used to swap fee tokens to stablecoin
pub const BRIDGES: Map<String, AssetInfo> = Map::new("bridges");

/// ## Description
/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
