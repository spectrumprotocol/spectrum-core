use cosmwasm_std::{Addr, CosmosMsg, QuerierWrapper, StdResult, to_binary, Uint128, WasmMsg};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Token(pub Addr);

impl Token {
    pub fn transfer_msg(&self, recipient: String, amount: Uint128) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient,
                amount,
            })?,
            funds: vec![],
        }))
    }

    pub fn burn_msg(&self, amount: Uint128) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Burn {
                amount,
            })?,
            funds: vec![],
        }))
    }

    pub fn query_balance(&self, querier: &QuerierWrapper, address: String) -> StdResult<Uint128> {
        let res: BalanceResponse = querier.query_wasm_smart(
            self.0.to_string(),
            &Cw20QueryMsg::Balance {
                address,
            })?;

        // load balance form the token contract
        Ok(res.balance)
    }
}
