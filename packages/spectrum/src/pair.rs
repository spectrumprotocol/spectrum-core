use astroport::pair::{Cw20HookMsg, ExecuteMsg, QueryMsg};
use astroport::{
    asset::{Asset, AssetInfo},
    pair::{PoolResponse, ReverseSimulationResponse, SimulationResponse},
};
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, Decimal, QuerierWrapper, QueryRequest, StdError, StdResult,
    Uint128, WasmMsg, WasmQuery,
};
use cw20::{Cw20ExecuteMsg, Expiration};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Pair(pub Addr);

impl Pair {
    pub fn swap_msg(
        &self,
        asset: &Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    ) -> StdResult<CosmosMsg> {
        let wasm_msg = match &asset.info {
            AssetInfo::Token { contract_addr } => WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: self.0.to_string(),
                    amount: asset.amount,
                    msg: to_binary(&Cw20HookMsg::Swap {
                        belief_price,
                        max_spread,
                        to,
                    })?,
                })?,
                funds: vec![],
            },

            AssetInfo::NativeToken { denom } => WasmMsg::Execute {
                contract_addr: self.0.to_string(),
                msg: to_binary(&ExecuteMsg::Swap {
                    offer_asset: asset.clone(),
                    belief_price,
                    max_spread,
                    to: None,
                })?,
                funds: vec![Coin {
                    denom: denom.clone(),
                    amount: asset.amount,
                }],
            },
        };

        Ok(CosmosMsg::Wasm(wasm_msg))
    }

    pub fn provide_msgs(
        &self,
        assets: &[Asset; 2],
        slippage_tolerance: Option<Decimal>,
        height: u64,
    ) -> StdResult<Vec<CosmosMsg>> {
        let mut msgs: Vec<CosmosMsg> = vec![];
        let mut funds: Vec<Coin> = vec![];

        for asset in assets.iter() {
            match &asset.info {
                AssetInfo::Token { contract_addr } => {
                    msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: contract_addr.to_string(),
                        msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                            spender: self.0.to_string(),
                            amount: asset.amount,
                            expires: Some(Expiration::AtHeight(height + 1)),
                        })?,
                        funds: vec![],
                    }))
                }
                AssetInfo::NativeToken { denom } => funds.push(Coin {
                    denom: denom.clone(),
                    amount: asset.amount,
                }),
            }
        }

        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&ExecuteMsg::ProvideLiquidity {
                assets: [assets[0].clone().into(), assets[1].clone().into()],
                slippage_tolerance,
                auto_stake: None,
                receiver: None,
            })?,
            funds,
        }));

        Ok(msgs)
    }

    pub fn query_pool(
        &self,
        querier: &QuerierWrapper,
        primary_asset_info: &AssetInfo,
        secondary_asset_info: &AssetInfo,
    ) -> StdResult<(Uint128, Uint128, Uint128)> {
        let response: PoolResponse = querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.0.to_string(),
            msg: to_binary(&QueryMsg::Pool {})?,
        }))?;

        let primary_asset_depth = response
            .assets
            .iter()
            .find(|asset| &asset.info == primary_asset_info)
            .ok_or_else(|| StdError::generic_err("Cannot find primary asset in pool response"))?
            .amount;

        let secondary_asset_depth = response
            .assets
            .iter()
            .find(|asset| &asset.info == secondary_asset_info)
            .ok_or_else(|| StdError::generic_err("Cannot find secondary asset in pool response"))?
            .amount;

        Ok((
            primary_asset_depth,
            secondary_asset_depth,
            response.total_share,
        ))
    }

    pub fn simulate(
        &self,
        querier: &QuerierWrapper,
        asset: &Asset,
    ) -> StdResult<SimulationResponse> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.0.to_string(),
            msg: to_binary(&QueryMsg::Simulation {
                offer_asset: asset.clone().into(),
            })?,
        }))
    }

    pub fn reverse_simulate(
        &self,
        querier: &QuerierWrapper,
        asset: &Asset,
    ) -> StdResult<ReverseSimulationResponse> {
        querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: self.0.to_string(),
            msg: to_binary(&QueryMsg::ReverseSimulation {
                ask_asset: asset.clone().into(),
            })?,
        }))
    }
}
