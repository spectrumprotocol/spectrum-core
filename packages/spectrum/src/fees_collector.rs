use astroport::asset::{Asset, AssetInfo};
use cosmwasm_std::{Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure stores general parameters for the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Address that's allowed to update bridge assets
    pub operator: String,
    /// The factory contract address
    pub factory_contract: String,
    /// The stablecoin asset info
    pub stablecoin: AssetInfo,
    /// The beneficiary address to received fees in stablecoin
    pub beneficiary: String,
    /// The maximum spread used when swapping fee tokens to stablecoin
    pub max_spread: Option<Decimal>,
}

/// This structure describes the functions that can be executed in this contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Collects and swaps fee tokens to stablecoin
    Collect {
        /// The assets to swap to stablecoin
        assets: Vec<AssetWithLimit>,
    },
    /// Add bridge tokens used to swap specific fee tokens to stablecoin (effectively declaring a swap route)
    UpdateBridges {
        add: Option<Vec<(AssetInfo, AssetInfo)>>,
        remove: Option<Vec<AssetInfo>>,
    },
    /// Swap fee tokens via bridge assets
    SwapBridgeAssets { assets: Vec<AssetInfo>, depth: u64 },
    /// Distribute stablecoin to beneficiary
    DistributeFees {},
}

/// This structure describes the query functions available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns information about the maker configs that contains in the [`ConfigResponse`]
    Config {},
    /// Returns the balance for each asset in the specified input parameters
    Balances {
        assets: Vec<AssetInfo>,
    },
    Bridges {},
}

/// A custom struct used to return multiple asset balances.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BalancesResponse {
    pub balances: Vec<Asset>,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

/// This struct holds parameters to help with swapping a specific amount of a fee token to ASTRO.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AssetWithLimit {
    /// Information about the fee token to swap
    pub info: AssetInfo,
    /// The amount of tokens to swap
    pub limit: Option<Uint128>,
}
