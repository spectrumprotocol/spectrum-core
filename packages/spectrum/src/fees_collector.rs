use astroport::asset::{Asset, AssetInfo};
use cosmwasm_std::{Addr, Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure stores general parameters for the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Address that's allowed to change contract parameters
    pub owner: String,
    /// The factory contract address
    pub factory_contract: String,
    /// The stablecoin token contract address
    pub stablecoin_token_contract: String,
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
    /// Updates general settings
    UpdateConfig {
        /// The factory contract address
        factory_contract: Option<String>,
        /// The beneficiary address
        beneficiary: Option<String>,
        /// The maximum spread used when swapping fee tokens to stablecoin
        max_spread: Option<Decimal>,
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
    /// Creates a request to change the contract's ownership
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the proposal to change the owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    DropOwnershipProposal {},
    /// Claims contract ownership
    ClaimOwnership {},
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

/// A custom struct that holds contract parameters and is used to retrieve them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    /// Address that is allowed to update contract parameters
    pub owner: Addr,
    /// The factory contract address
    pub factory_contract: Addr,
    /// The stablecoin token contract address
    pub stablecoin_token_contract: Addr,
    /// The beneficiary address to received fees in stablecoin
    pub beneficiary: Addr,
    /// The maximum spread used when swapping fee tokens to ASTRO
    pub max_spread: Decimal,
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
