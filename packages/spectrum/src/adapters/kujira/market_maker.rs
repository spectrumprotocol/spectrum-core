use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Coin, CosmosMsg, CustomQuery, Decimal, QuerierWrapper, StdResult, to_binary, Uint128, WasmMsg};
use kujira::denom::Denom;
use kujira::precision::Precision;

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: Addr,
    pub fin_contract: Addr,
    pub intervals: Vec<Decimal>,
    pub fee: Decimal,
    pub amp: Decimal,
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateConfig {
        owner: Option<Addr>,
        intervals: Option<Vec<Decimal>>,
        fee: Option<Decimal>,
        amp: Option<Decimal>,
    },
    Run {},
    Deposit {
        max_slippage: Option<Decimal>,
        /// Optionally add a submsg that is called when the LP tokens are minted, used for auto-stake
        callback: Option<Callback>,
    },
    Withdraw {},
}

#[cw_serde]
pub struct Callback {
    pub contract_addr: String,
    pub msg: Binary,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    /// A shortcut for totalling both balances
    #[returns(PoolResponse)]
    Pool {},

    #[returns(kujira::fin::OrdersResponse)]
    Orders {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub owner: Addr,
    pub denoms: [Denom; 2],
    pub price_precision: Precision,
    pub decimal_delta: i8,
    pub fin_contract: Addr,
    pub intervals: Vec<Decimal>,
    pub fee: Decimal,
    pub amp: Decimal,
}

#[cw_serde]
pub struct PoolResponse {
    pub balances: [Uint128; 2],
}

#[cw_serde]
pub struct MigrateMsg {
    pub intervals: Option<Vec<Decimal>>,
}

#[cw_serde]
pub struct MarketMaker(pub Addr);

impl MarketMaker {
    pub fn deposit(
        &self,
        funds: Vec<Coin>,
        max_slippage: Option<Decimal>,
        callback: Option<Callback>
    ) -> StdResult<CosmosMsg> {
        let wasm_msg = WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&ExecuteMsg::Deposit {
                max_slippage,
                callback
            })?,
            funds,
        };

        Ok(CosmosMsg::Wasm(wasm_msg))
    }

    pub fn withdraw(
        &self,
        lp: Coin,
    ) -> StdResult<CosmosMsg> {
        let wasm_msg = WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&ExecuteMsg::Withdraw {})?,
            funds: vec![lp],
        };

        Ok(CosmosMsg::Wasm(wasm_msg))
    }

    pub fn query_config<C: CustomQuery>(
        &self,
        querier: &QuerierWrapper<C>,
    ) -> StdResult<ConfigResponse> {
        querier.query_wasm_smart(self.0.to_string(), &QueryMsg::Config {})
    }

    pub fn query_pool<C: CustomQuery>(
        &self,
        querier: &QuerierWrapper<C>,
    ) -> StdResult<PoolResponse> {
        querier.query_wasm_smart(self.0.to_string(), &QueryMsg::Pool {})
    }

}