use cosmwasm_std::{Addr, Coin, CosmosMsg, Decimal256, QuerierWrapper, StdResult, to_binary, WasmMsg};
use kujira::asset::Asset;
use kujira::fin::{ConfigResponse, ExecuteMsg, QueryMsg, SimulationResponse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Pair(pub Addr);

impl Pair {
    pub fn query_config(&self, querier: &QuerierWrapper) -> StdResult<ConfigResponse> {
        querier.query_wasm_smart(self.0.to_string(), &QueryMsg::Config {})
    }

    pub fn simulate(
        &self,
        querier: &QuerierWrapper,
        offer_asset: &Asset,
    ) -> StdResult<SimulationResponse> {
        querier.query_wasm_smart(self.0.to_string(), &QueryMsg::Simulation {
            offer_asset: offer_asset.clone(),
        })
    }

    /// Generate msg for swapping specified asset
    pub fn swap_msg(
        &self,
        asset: Coin,
        belief_price: Option<Decimal256>,
        max_spread: Option<Decimal256>,
        to: Option<Addr>,
    ) -> StdResult<CosmosMsg> {
        let wasm_msg = WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&ExecuteMsg::Swap {
                offer_asset: Some(asset.clone()),
                belief_price,
                max_spread,
                to,
            })?,
            funds: vec![asset],
        };

        Ok(CosmosMsg::Wasm(wasm_msg))
    }
}