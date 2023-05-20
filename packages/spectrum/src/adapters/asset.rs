use cosmwasm_std::{Addr, BankMsg, Coin, CosmosMsg, MessageInfo, StdError, StdResult, to_binary, WasmMsg};
use cw20::{Cw20ExecuteMsg, Expiration};
use astroport::asset::{Asset, AssetInfo};

pub trait AssetEx {

    fn transfer_msg(&self, to: &Addr) -> StdResult<CosmosMsg>;
    fn transfer_from_msg(&self, from: &Addr, to: &Addr) -> StdResult<CosmosMsg>;
    fn increase_allowance_msg(&self, spender: String, expires: Option<Expiration>) -> StdResult<CosmosMsg>;

    fn deposit_asset(
        &self,
        info: &mut MessageInfo,
        recipient: &Addr,
        messages: &mut Vec<CosmosMsg>,
    ) -> StdResult<()>;
}

impl AssetEx for Asset {

    fn transfer_msg(
        &self,
        to: &Addr,
    ) -> StdResult<CosmosMsg> {
        match &self.info {
            AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: to.to_string(),
                    amount: self.amount,
                })?,
                funds: vec![],
            })),
            AssetInfo::NativeToken { denom } => Ok(CosmosMsg::Bank(BankMsg::Send {
                to_address: to.to_string(),
                amount: vec![Coin {
                    denom: denom.to_string(),
                    amount: self.amount,
                }],
            })),
        }
    }

    fn transfer_from_msg(&self, from: &Addr, to: &Addr) -> StdResult<CosmosMsg> {
        match &self.info {
            AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: from.to_string(),
                    recipient: to.to_string(),
                    amount: self.amount,
                })?,
                funds: vec![],
            })),
            AssetInfo::NativeToken { .. } => Err(StdError::generic_err(
                "TransferFrom does not apply to native tokens",
            )),
        }
    }

    fn increase_allowance_msg(&self, spender: String, expires: Option<Expiration>) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.info.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender,
                amount: self.amount,
                expires,
            })?,
            funds: vec![],
        }))
    }

    fn deposit_asset(
        &self,
        info: &mut MessageInfo,
        recipient: &Addr,
        messages: &mut Vec<CosmosMsg>,
    ) -> StdResult<()> {

        if self.amount.is_zero() {
            return Ok(());
        }

        match &self.info {
            AssetInfo::Token { .. } => {
                messages.push(self.transfer_from_msg(&info.sender, recipient)?);
            }
            AssetInfo::NativeToken { denom } => {
                let coin = info.funds.iter_mut().find(|it| it.denom.eq(denom));
                match coin {
                    Some(coin) => {
                        if coin.amount != self.amount {
                            return Err(StdError::generic_err(
                                "Native token balance mismatch between the argument and the transferred",
                            ));
                        }
                        coin.amount -= self.amount;
                    },
                    None => return Err(StdError::generic_err(format!("Must send reserve token '{0}'", denom))),
                }
            }
        };
        Ok(())
    }

}
