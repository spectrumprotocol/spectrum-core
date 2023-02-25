use schemars::{JsonSchema};
use serde::{Deserialize, Serialize};

use cosmwasm_std::{to_binary, Addr, CosmosMsg, StdResult, WasmMsg, Decimal, Uint128, Coin, Decimal256};
use crate::adapters::kujira::market_maker::MarketMaker;
use crate::adapters::pair::Pair;

/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// Router to swap reward token to the asset in the pair
    pub router: String,
}

/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Compound rewards to LP token
    Compound {
        market_maker: String,
        /// Skip optimal swap
        no_swap: Option<bool>,
        /// slippage tolerance when providing LP
        slippage_tolerance: Option<Decimal>,
    },
    /// The callback of type [`CallbackMsg`]
    Callback(CallbackMsg),
}

/// This structure describes the callback messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    /// Performs optimal swap
    OptimalSwap {
        pair: Pair,
        market_maker: MarketMaker,
        prev_balances: [Coin; 2],
        slippage_tolerance: Option<Decimal256>,
    },
    /// Provides liquidity to the pair contract
    ProvideLiquidity {
        market_maker: MarketMaker,
        prev_balances: [Coin; 2],
        slippage_tolerance: Option<Decimal>,
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
        market_maker: String,
        rewards: Vec<Coin>,
    },
}

/// This structure holds the parameters that are returned from a compound simulation response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CompoundSimulationResponse {
    /// The amount of LP returned from compound
    pub lp_amount: Uint128,
    /// The amount of asset A to be swapped
    pub swap_asset_a_amount: Uint128,
    /// The amount of asset B to be swapped
    pub swap_asset_b_amount: Uint128,
    /// The amount of asset A returned from swap
    pub return_a_amount: Uint128,
    /// The amount of asset B returned from swap
    pub return_b_amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Compounder(pub Addr);

impl Compounder {
    pub fn compound_msg<T>(&self, market_maker: String, mut funds: Vec<Coin>, no_swap: Option<bool>, slippage_tolerance: Option<Decimal>) -> StdResult<CosmosMsg<T>> {
        funds.sort_by(|a, b| a.denom.cmp(&b.denom));
        Ok(CosmosMsg::<T>::Wasm(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&ExecuteMsg::Compound {
                market_maker,
                no_swap,
                slippage_tolerance,
            })?,
            funds,
        }))
    }
}
