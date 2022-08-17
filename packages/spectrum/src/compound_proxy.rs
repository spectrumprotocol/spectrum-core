use schemars::{JsonSchema};
use serde::{Deserialize, Serialize};

use astroport::asset::{Asset, AssetInfo};

use cosmwasm_std::{to_binary, Addr, CosmosMsg, StdResult, WasmMsg, Decimal, Uint128, Coin};

/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// The pair contract address
    pub pair_contract: String,
    /// The swap commission
    pub commission_bps: u64,
    /// The list of pair proxy to swap reward token to the asset in the pair
    pub pair_proxies: Vec<(AssetInfo, String)>,
    /// The slippage tolerance when swapping
    pub slippage_tolerance: Decimal,
}

/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Compound rewards to LP token
    Compound {
        /// List of reward asset send to compound
        rewards: Vec<Asset>,
        /// Receiver address for LP token
        to: Option<String>,
    },
    /// The callback of type [`CallbackMsg`]
    Callback(CallbackMsg),
}

/// This structure describes the callback messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    /// Performs optimal swap
    OptimalSwap {},
    /// Provides liquidity to the pair contract
    ProvideLiquidity {
        receiver: String,
    },
}

// Modified from
// https://github.com/CosmWasm/cw-plus/blob/v0.8.0/packages/cw20/src/receiver.rs#L23
impl CallbackMsg {
    pub fn into_cosmos_msg(&self, contract_addr: &Addr) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from(contract_addr),
            msg: to_binary(&ExecuteMsg::Callback(self.clone()))?,
            funds: vec![],
        }))
    }
}

/// This structure describes the query messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns controls settings that specified in custom [`ConfigResponse`] structure.
    Config {},
    /// Return LP token amount received after compound
    CompoundSimulation {
        rewards: Vec<Asset>,
    },
}

/// This structure holds the parameters that are returned from a compound simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CompoundSimulationResponse {
    /// The amount of LP returned from compound
    pub lp_amount: Uint128,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Compounder(pub Addr);

impl Compounder {
    pub fn compound_msg(&self, rewards: Vec<Asset>, funds: Vec<Coin>) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&ExecuteMsg::Compound {
                rewards,
                to: None,
            })?,
            funds,
        }))
    }
}
