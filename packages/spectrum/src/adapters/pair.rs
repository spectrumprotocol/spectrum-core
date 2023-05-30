use cosmwasm_std::{Addr, Coin, CosmosMsg, CustomQuery, Decimal256, QuerierWrapper, StdResult, to_binary, WasmMsg};
use kujira::asset::Asset;
use kujira::fin::{ExecuteMsg, QueryMsg, SimulationResponse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Pair(pub Addr);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponseCustom {
    /// See [InstantiateMsg::owner]
    pub owner: Addr,

    /// See [InstantiateMsg::denoms]
    pub denoms: [cw20::Denom; 2],

    /// See [InstantiateMsg::price_precision]
    pub price_precision: kujira::precision::Precision,

    /// See [InstantiateMsg::decimal_delta]
    pub decimal_delta: Option<i8>,

    /// When a book is bootstrapping, it can accept orders but trades are not yet executed
    pub is_bootstrapping: bool,

    /// See [InstantiateMsg::fee_taker]    
    pub fee_taker: Option<Decimal256>,

    /// See [InstantiateMsg::fee_maker]
    pub fee_maker: Option<Decimal256>,

    /// See [InstantiateMsg::fee_maker_negative]
    pub fee_maker_negative: Option<bool>,
}

impl Pair {
    pub fn query_config<C: CustomQuery>(&self, querier: &QuerierWrapper<C>) -> StdResult<ConfigResponseCustom> {
        querier.query_wasm_smart(self.0.to_string(), &QueryMsg::Config {})
    }

    pub fn simulate<C: CustomQuery>(
        &self,
        querier: &QuerierWrapper<C>,
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