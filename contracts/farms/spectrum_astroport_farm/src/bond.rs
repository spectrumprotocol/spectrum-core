use astroport::asset::{addr_validate_to_lower, Asset};
use astroport::querier::query_token_balance;
use cosmwasm_std::{
    attr, to_binary, Addr, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    Uint128, WasmMsg, Coin,
};

use crate::error::ContractError;
use crate::state::{RewardInfo, ScalingOperation, State, CONFIG, REWARD, STATE};

use cw20::Cw20ExecuteMsg;

use crate::querier::query_astroport_pool_balance;
use astroport::generator::{
    Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportExecuteMsg,
};
use spectrum::astroport_farm::{RewardInfoResponse, RewardInfoResponseItem, CallbackMsg, Cw20HookMsg};
use spectrum::compound_proxy::ExecuteMsg as CompoundProxyExecuteMsg;
use spectrum::farm_helper::{compute_deposit_time, deposit_asset};

pub fn bond_assets(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    minimum_receive: Option<Uint128>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let staking_token = config.pair_info.liquidity_token;

    let mut messages: Vec<CosmosMsg> = vec![];
    
    let mut funds: Vec<Coin> = vec![];
    
    for asset in assets.clone() {
        deposit_asset(&env, &info, &mut messages, &asset)?;
        if asset.is_native_token() {
            funds.push(Coin {
                denom: asset.info.to_string(),
                amount: asset.amount,
            });
        }
    }

    let compound = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.compound_proxy.to_string(),
        msg: to_binary(&CompoundProxyExecuteMsg::Compound {
            rewards: assets,
            to: None,
        })?,
        funds,
    });
    messages.push(compound);

    let prev_balance = query_token_balance(&deps.querier, staking_token, env.contract.address.clone())?;
    messages.push(
        CallbackMsg::BondTo {
            to: info.sender.to_string(),
            prev_balance,
            minimum_receive,
        }
        .into_cosmos_msg(&env.contract.address)?,
    );

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "bond_assets"))
}

pub fn bond_to(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    to: String,
    prev_balance: Uint128,
    minimum_receive: Option<Uint128>
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let staking_token = config.pair_info.liquidity_token;

    let balance = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address.clone())?;
    let amount = balance - prev_balance;

    if let Some(minimum_receive) = minimum_receive {
        if amount < minimum_receive {
            return Err(ContractError::AssertionMinimumReceive {
                minimum_receive,
                amount,
            });
        }
    }

    Ok(Response::new()
    .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: staking_token.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: env.contract.address.to_string(),
            amount,
            msg: to_binary(&Cw20HookMsg::Bond { staker_addr: Some(to) })?,
        })?,
    })])
    .add_attributes(vec![
        attr("action", "bond_to"),
        attr("amount", amount),
    ]))

}

pub fn bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender_addr: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staker_addr = addr_validate_to_lower(deps.api, &sender_addr)?;

    let config = CONFIG.load(deps.storage)?;
    let staking_token = config.pair_info.liquidity_token;

    // only staking token contract can execute this message
    if staking_token != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let config = CONFIG.load(deps.storage)?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &staking_token,
        &env.contract.address,
        &config.staking_contract,
    )?;

    let mut messages: Vec<CosmosMsg> = vec![];

    let mut state = STATE.load(deps.storage)?;

    // withdraw reward to pending reward; before changing share
    let mut reward_info = REWARD
        .may_load(deps.storage, &staker_addr)?
        .unwrap_or_else(RewardInfo::create);

    let deposit_amount = increase_bond_amount(&mut state, &mut reward_info, amount, lp_balance);

    let last_deposit_amount = reward_info.deposit_amount;
    reward_info.deposit_amount = last_deposit_amount + deposit_amount;
    reward_info.deposit_time = compute_deposit_time(
        last_deposit_amount,
        deposit_amount,
        reward_info.deposit_time,
        env.block.time.seconds(),
    )?;

    REWARD.save(deps.storage, &staker_addr, &reward_info)?;
    STATE.save(deps.storage, &state)?;

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: staking_token.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: config.staking_contract.to_string(),
            amount,
            msg: to_binary(&AstroportCw20HookMsg::Deposit {})?,
        })?,
    }));
    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "bond"),
        attr("lp_token", staking_token.to_string()),
        attr("amount", amount),
        attr("bond_amount", amount),
    ]))
}

// increase share amount in pool and reward info
fn increase_bond_amount(
    state: &mut State,
    reward_info: &mut RewardInfo,
    bond_amount: Uint128,
    lp_balance: Uint128,
) -> Uint128 {
    // convert amount to share & update
    let bond_share = state.calc_bond_share(bond_amount, lp_balance, ScalingOperation::Truncate);
    state.total_bond_share += bond_share;
    reward_info.bond_share += bond_share;

    state.calc_user_balance(
        lp_balance + bond_amount,
        bond_share,
        ScalingOperation::Truncate,
    )
}

pub fn unbond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staker_addr = info.sender;

    let config = CONFIG.load(deps.storage)?;
    let staking_token = config.pair_info.liquidity_token;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &staking_token,
        &env.contract.address,
        &config.staking_contract,
    )?;

    let mut state = STATE.load(deps.storage)?;
    let mut reward_info = REWARD.load(deps.storage, &staker_addr)?;

    let user_balance = state.calc_user_balance(
        lp_balance,
        reward_info.bond_share,
        ScalingOperation::Truncate,
    );

    if user_balance < amount {
        return Err(ContractError::UnbondExceedBalance {});
    }

    let bond_share = state.calc_bond_share(amount, lp_balance, ScalingOperation::Ceil);

    state.total_bond_share = state.total_bond_share.checked_sub(bond_share)?;
    reward_info.bond_share = reward_info.bond_share.checked_sub(bond_share)?;

    reward_info.deposit_amount = reward_info
        .deposit_amount
        .multiply_ratio(user_balance.checked_sub(amount)?, user_balance);

    // update state
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.staking_contract.to_string(),
                funds: vec![],
                msg: to_binary(&AstroportExecuteMsg::Withdraw {
                    lp_token: staking_token.to_string(),
                    amount,
                })?,
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: staking_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: staker_addr.to_string(),
                    amount,
                })?,
                funds: vec![],
            }),
        ])
        .add_attributes(vec![
            attr("action", "unbond"),
            attr("staker_addr", staker_addr),
            attr("amount", amount),
        ]))
}

pub fn query_reward_info(
    deps: Deps,
    env: Env,
    staker_addr: String,
) -> StdResult<RewardInfoResponse> {
    let staker_addr_validated = addr_validate_to_lower(deps.api, &staker_addr)?;
    let reward_info = read_reward_info(deps, env, &staker_addr_validated)?;

    Ok(RewardInfoResponse {
        staker_addr,
        reward_info,
    })
}

fn read_reward_info(deps: Deps, env: Env, staker_addr: &Addr) -> StdResult<RewardInfoResponseItem> {
    let reward_info = REWARD
        .load(deps.storage, staker_addr)
        .unwrap_or(RewardInfo {
            bond_share: Uint128::zero(),
            deposit_amount: Uint128::zero(),
            deposit_cost: Uint128::zero(),
            deposit_time: 0,
        });
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let staking_token = config.pair_info.liquidity_token;

    let lp_balance = query_astroport_pool_balance(
        deps,
        &staking_token,
        &env.contract.address,
        &config.staking_contract,
    )?;

    let bond_amount = state.calc_user_balance(
        lp_balance,
        reward_info.bond_share,
        ScalingOperation::Truncate,
    );
    Ok(RewardInfoResponseItem {
        staking_token: staking_token.to_string(),
        bond_share: reward_info.bond_share,
        bond_amount,
        deposit_amount: reward_info.deposit_amount,
        deposit_time: reward_info.deposit_time,
    })
}