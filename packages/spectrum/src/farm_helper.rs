use astroport::asset::{Asset, AssetInfo};
use astroport::pair::PoolResponse;
use cosmwasm_std::{StdError, StdResult, Uint128, Addr, CosmosMsg, WasmMsg, to_binary, Env, MessageInfo};
use cw20::Cw20ExecuteMsg;
use std::convert::TryFrom;

pub fn compute_deposit_time(
    last_deposit_amount: Uint128,
    new_deposit_amount: Uint128,
    last_deposit_time: u64,
    new_deposit_time: u64,
) -> StdResult<u64> {
    let last_weight = last_deposit_amount.u128() * (last_deposit_time as u128);
    let new_weight = new_deposit_amount.u128() * (new_deposit_time as u128);
    let weight_avg =
        (last_weight + new_weight) / (last_deposit_amount.u128() + new_deposit_amount.u128());
    u64::try_from(weight_avg).map_err(|_| StdError::generic_err("Overflow in compute_deposit_time"))
}

pub fn compute_provide_after_swap(
    pool: &PoolResponse,
    offer: &Asset,
    return_amt: Uint128,
    ask_reinvest_amt: Uint128,
) -> StdResult<Uint128> {
    let (offer_amount, ask_amount) = if pool.assets[0].info == offer.info {
        (pool.assets[0].amount, pool.assets[1].amount)
    } else {
        (pool.assets[1].amount, pool.assets[0].amount)
    };

    let offer_amount = offer_amount + offer.amount;
    let ask_amount = ask_amount.checked_sub(return_amt)?;

    Ok(ask_reinvest_amt.multiply_ratio(offer_amount, ask_amount))
}

pub fn deposit_asset(
    env: &Env,
    info: &MessageInfo,
    messages: &mut Vec<CosmosMsg>,
    asset: &Asset,
) -> StdResult<()> {
    if asset.amount.is_zero() {
        return Ok(());
    }

    match asset.info {
        AssetInfo::Token { .. } => {
            messages.push(transfer_from_msg(
                asset,
                &info.sender,
                &env.contract.address,
            )?);
            Ok(())
        }
        AssetInfo::NativeToken { .. } => {
            asset.assert_sent_native_token_balance(info)?;
            Ok(())
        }
    }
}

pub fn transfer_from_msg(asset: &Asset, from: &Addr, to: &Addr) -> StdResult<CosmosMsg> {
    match &asset.info {
        AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: from.to_string(),
                recipient: to.to_string(),
                amount: asset.amount,
            })?,
            funds: vec![],
        })),
        AssetInfo::NativeToken { .. } => Err(StdError::generic_err(
            "TransferFrom does not apply to native tokens",
        )),
    }
}