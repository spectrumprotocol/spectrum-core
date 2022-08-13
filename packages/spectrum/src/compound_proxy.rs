use schemars::{JsonSchema};
use serde::{Deserialize, Serialize};

use astroport::asset::{Asset, AssetInfo};

use cosmwasm_std::{to_binary, Addr, CosmosMsg, StdResult, WasmMsg, Decimal, Coin};
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// the pair contract address
    pub pair_contract: String,
    /// the swap commission
    pub commission_bps: u64,
    pub pair_proxies: Vec<(AssetInfo, String)>,
    pub slippage_tolerance: Decimal,
}

/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Compound rewards to staking token
    Compound {
        rewards: Vec<Asset>,
        to: Option<String>,
    },
    /// the callback of type [`CallbackMsg`]
    Callback(CallbackMsg),
}

/// This structure describes the callback messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    OptimalSwap {},
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
