use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::asset::{Asset, AssetInfo};

use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;

/// the default slippage
pub const DEFAULT_SLIPPAGE: &str = "0.005";
/// the maximum allowed slippage
pub const MAX_ALLOWED_SLIPPAGE: &str = "0.5";

pub const TWAP_PRECISION: u8 = 6;

pub const MAX_SWAP_OPERATIONS: usize = 50;

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// the type of asset infos available in [`AssetInfo`]
    pub asset_infos: [AssetInfo; 2],
    /// the factory contract address
    pub factory_addr: String,
    /// swap operations for this pair
    pub operations: Vec<SwapOperation>
}

/// ## Description
/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// ## Description
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
    /// Swap an offer asset to the other
    Swap {
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
    /// Update pair config if required
    UpdateConfig { operations: Vec<SwapOperation>, },
    /// Internal use
    /// Swap all offer tokens to ask token
    ExecuteSwapOperation {
        operation: SwapOperation,
        to: Option<String>,
        max_spread: Option<Decimal>,
    },
    /// Internal use
    /// Check the swap amount is exceed minimum_receive
    AssertMinimumReceive {
        asset_info: AssetInfo,
        prev_balance: Uint128,
        minimum_receive: Uint128,
        receiver: String,
    },
}

/// ## Description
/// This structure describes a CW20 hook message.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    /// Sell a given amount of asset
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    },
}

/// ## Description
/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns information about a pair in an object of type [`PairInfo`].
    Pair {},
    /// Returns information about the simulation of the swap in a [`SimulationResponse`] object.
    Simulation { offer_asset: Asset },
    /// Returns information about the reverse simulation in a [`ReverseSimulationResponse`] object.
    ReverseSimulation { ask_asset: Asset },
    /// Returns controls settings that specified in custom [`ConfigResponse`] structure.
    Config {},
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolResponse {
    pub assets: [Asset; 2],
    pub total_share: Uint128,
}

/// ## Description
/// This structure describes the custom struct for each query response.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub operations: Vec<SwapOperation>,
}

/// ## Description
/// SimulationResponse returns swap simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SimulationResponse {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
}

/// ## Description
/// ReverseSimulationResponse returns reverse swap simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ReverseSimulationResponse {
    pub offer_amount: Uint128,
    pub spread_amount: Uint128,
    pub commission_amount: Uint128,
}

/// ## Description
/// This enum describes the swap operation.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SwapOperation {
    /// Native swap
    NativeSwap {
        /// the offer denom
        offer_denom: String,
        /// the asks denom
        ask_denom: String,
    },
    /// ASTRO swap
    AstroSwap {
        /// the offer asset info
        offer_asset_info: AssetInfo,
        /// the asks asset info
        ask_asset_info: AssetInfo,
    },
}

impl SwapOperation {
    pub fn get_target_asset_info(&self) -> AssetInfo {
        match self {
            SwapOperation::NativeSwap { ask_denom, .. } => AssetInfo::NativeToken {
                denom: ask_denom.clone(),
            },
            SwapOperation::AstroSwap { ask_asset_info, .. } => ask_asset_info.clone(),
        }
    }
    pub fn get_offer_asset_info(&self) -> AssetInfo {
        match self {
            SwapOperation::NativeSwap { offer_denom, .. } => AssetInfo::NativeToken {
                denom: offer_denom.clone(),
            },
            SwapOperation::AstroSwap { offer_asset_info, .. } => offer_asset_info.clone(),
        }
    }
}

/// ## Description
/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}