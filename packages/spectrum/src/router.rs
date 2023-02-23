use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Api, Coin, CosmosMsg, CustomQuery, Decimal256, QuerierWrapper, StdResult, to_binary, Uint128, WasmMsg};
use kujira::asset::{Asset, AssetInfo};
use kujira::denom::Denom;
use kujira::fin::SimulationResponse;
use crate::adapters::pair::Pair;

/// Maximum assets in the swap route
pub const MAX_ASSETS: usize = 50;

/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SwapOperationRequest {
    pub pair: String,
    pub offer: Denom,
    pub ask: Denom,
}

impl SwapOperationRequest {
    pub fn validate(&self, api: &dyn Api) -> StdResult<SwapOperation> {
        Ok(SwapOperation {
            pair: Pair(api.addr_validate(&self.pair)?),
            offer: self.offer.clone(),
            ask: self.ask.clone(),
        })
    }
}

/// This structure describes the execute messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // admin
    UpsertRoute {
        operations: Vec<SwapOperationRequest>
    },
    RemoveRoute {
        denoms: [Denom; 2]
    },
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

    // any
    /// Swap an offer asset to the other
    Swap {
        belief_price: Option<Decimal256>,
        max_spread: Option<Decimal256>,
        to: Option<String>,
        ask: Denom,
    },
    /// ExecuteSwapOperations processes multiple swaps while mentioning the minimum amount of tokens to receive for the last swap operation
    ExecuteSwapOperations {
        operations: Vec<SwapOperationRequest>,
        minimum_receive: Option<Uint128>,
        to: Option<String>,
        max_spread: Option<Decimal256>,
    },

    // self
    /// The callback of type [`CallbackMsg`]
    Callback(CallbackMsg),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct SwapOperation {
    pub pair: Pair,
    pub offer: Denom,
    pub ask: Denom,
}

impl SwapOperation {
    pub fn rev(&self) -> SwapOperation {
        SwapOperation {
            pair: self.pair.clone(),
            offer: self.ask.clone(),
            ask: self.offer.clone(),
        }
    }

    pub fn get_key_compat(&self, offer: &cw20::Denom, ask: &cw20::Denom) -> (bool, bool) {
        let offer_denom = match offer {
            cw20::Denom::Native(denom) => denom,
            cw20::Denom::Cw20(_) => return (false, false),
        };
        let ask_denom = match ask {
            cw20::Denom::Native(denom) => denom,
            cw20::Denom::Cw20(_) => return (false, false),
        };

        if self.offer.eq(&offer_denom.into()) && self.ask.eq(&ask_denom.into()) {
            (true, false)
        } else if self.ask.eq(&offer_denom.into()) && self.offer.eq(&ask_denom.into()) {
            (true, true)
        } else {
            (false, false)
        }
    }
}

/// This structure describes the callback messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    Swap {
        previous_balance: Coin,
        operations: Vec<SwapOperation>,
        to: Addr,
        minimum_receive: Option<Uint128>,
        max_spread: Option<Decimal256>,
    },
}

// Modified from
// https://github.com/CosmWasm/cw-plus/blob/v0.8.0/packages/cw20/src/receiver.rs#L23
impl CallbackMsg {
    pub fn to_cosmos_msg(&self, contract_addr: &Addr) -> StdResult<CosmosMsg> {
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
    /// Returns controls settings that specified in [`Config`] structure.
    Config {},
    /// Returns route
    Route {
        denoms: [Denom; 2]
    },
    /// Returns routes
    Routes {
        start_after: Option<String>,
        limit: Option<u8>,
    },
    /// Returns information about a swap simulation in a [`SimulationResponse`] object.
    Simulation {
        /// Offer asset
        offer_asset: Asset,
        ask: Denom,
    },
    /// SimulateSwapOperations simulates multi-hop swap operations
    SimulateSwapOperations {
        /// The amount of tokens to swap
        offer_amount: Uint128,
        /// The swap operations to perform, each swap involving a specific pool
        operations: Vec<SwapOperationRequest>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Route {
    pub key: String,
    pub operations: Vec<SwapOperation>,
    pub decimal_delta: i8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Router(pub Addr);

impl Router {

    pub fn query_route<C: CustomQuery>(&self, querier: &QuerierWrapper<C>, denoms: [Denom; 2]) -> StdResult<Route> {
        querier.query_wasm_smart(self.0.to_string(), &QueryMsg::Route { denoms })
    }

    pub fn simulate<C: CustomQuery>(
        &self,
        querier: &QuerierWrapper<C>,
        offer_asset: Asset,
        ask: Denom,
    ) -> StdResult<SimulationResponse> {
        querier.query_wasm_smart(self.0.to_string(), &QueryMsg::Simulation {
            offer_asset,
            ask,
        })
    }

    pub fn swap_msg(
        &self,
        asset: Coin,
        ask: Denom,
        belief_price: Option<Decimal256>,
        max_spread: Option<Decimal256>,
        to: Option<String>,
    ) -> StdResult<CosmosMsg> {
        let wasm_msg = WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&ExecuteMsg::Swap {
                ask,
                belief_price,
                max_spread,
                to,
            })?,
            funds: vec![asset],
        };

        Ok(CosmosMsg::Wasm(wasm_msg))
    }

    pub fn try_build_swap_msg<C: CustomQuery>(
        &self,
        querier: &QuerierWrapper<C>,
        from: Denom,
        to: Denom,
        amount: Uint128,
    ) -> StdResult<CosmosMsg> {
        self.query_route(querier, [from.clone(), to.clone()])?;
        let msg = self.swap_msg(
            Coin { denom: from.to_string(), amount },
            to,
            None,
            None,
            None,
        )?;
        Ok(msg)
    }

    pub fn try_swap_simulation<C: CustomQuery>(
        &self,
        querier: &QuerierWrapper<C>,
        from: String,
        to: Denom,
        amount: Uint128,
    ) -> StdResult<Uint128> {
        let result = self.simulate(
            querier,
            Asset { info: AssetInfo::NativeToken { denom: from.into() }, amount },
            to,
        )?;
        Ok(result.return_amount.try_into()?)
    }
}
