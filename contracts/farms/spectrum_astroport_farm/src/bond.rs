use astroport::asset::addr_validate_to_lower;
use cosmwasm_std::{
    attr, to_binary, Addr, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128, WasmMsg,
};

use crate::error::ContractError;
use crate::state::{Config, PoolInfo, RewardInfo, CONFIG, POOL_INFO, REWARD};

use cw20::Cw20ExecuteMsg;

use crate::querier::query_astroport_pool_balance;
use astroport::generator::{
    Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportExecuteMsg,
};
use spectrum::astroport_farm::{RewardInfoResponse, RewardInfoResponseItem};
use spectrum::farm_helper::compute_deposit_time;

pub fn bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender_addr: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staker_addr = addr_validate_to_lower(deps.api, &sender_addr)?;

    let pool_info = POOL_INFO.load(deps.storage)?;

    // only staking token contract can execute this message
    if pool_info.staking_token != addr_validate_to_lower(deps.api, info.sender.as_str())? {
        return Err(ContractError::Unauthorized {});
    }

    let config = CONFIG.load(deps.storage)?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let mut messages: Vec<CosmosMsg> = vec![];

    let mut pool_info = POOL_INFO.load(deps.storage)?;

    // withdraw reward to pending reward; before changing share
    let mut reward_info = REWARD
        .may_load(deps.storage, &staker_addr)?
        .unwrap_or_else(|| RewardInfo::create());

    if reward_info.deposit_amount.is_zero() && (!reward_info.bond_share.is_zero()) {
        reward_info.deposit_amount = amount;
        reward_info.deposit_time = env.block.time.seconds();
        
        //TODO: deposit cost
    }

    let deposit_amount =
        increase_bond_amount(&mut pool_info, &mut reward_info, amount, lp_balance);

    let last_deposit_amount = reward_info.deposit_amount;
    reward_info.deposit_amount = last_deposit_amount + deposit_amount;
    reward_info.deposit_time = compute_deposit_time(
        last_deposit_amount,
        deposit_amount,
        reward_info.deposit_time,
        env.block.time.seconds(),
    )?;

    REWARD.save(deps.storage, &staker_addr, &reward_info)?;
    POOL_INFO.save(deps.storage, &pool_info)?;

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: pool_info.staking_token.to_string(),
        funds: vec![],
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: config.astroport_generator.to_string(),
            amount,
            msg: to_binary(&AstroportCw20HookMsg::Deposit {})?,
        })?,
    }));
    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "bond"),
        attr("lp_token", pool_info.staking_token.to_string()),
        attr("amount", amount),
        attr("bond_amount", amount),
    ]))
}

// increase share amount in pool and reward info
fn increase_bond_amount(
    pool_info: &mut PoolInfo,
    reward_info: &mut RewardInfo,
    bond_amount: Uint128,
    lp_balance: Uint128,
) -> Uint128 {
    // convert amount to share & update
    let bond_share = pool_info.calc_bond_share(bond_amount, lp_balance);
    pool_info.total_bond_share += bond_share;
    reward_info.bond_share += bond_share;

    let new_bond_amount = pool_info.calc_user_balance(lp_balance + bond_amount, bond_share);
    new_bond_amount
}

pub fn unbond(deps: DepsMut, env: Env, info: MessageInfo, amount: Uint128) -> Result<Response, ContractError> {
    let staker_addr = info.sender;

    let config = CONFIG.load(deps.storage)?;
    let pool_info = POOL_INFO.load(deps.storage)?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let mut pool_info = POOL_INFO.load(deps.storage)?;
    let mut reward_info = REWARD.load(deps.storage, &staker_addr)?;

    let user_balance = pool_info.calc_user_balance(lp_balance, reward_info.bond_share);

    if user_balance < amount {
        return Err(ContractError::UnbondExceedBalance {});
    }

    // add 1 to share, otherwise there will always be a fraction
    let mut bond_share = pool_info.calc_bond_share(amount, lp_balance);
    if pool_info.calc_user_balance(lp_balance, bond_share) < amount {
        bond_share += Uint128::new(1u128);
    }

    pool_info.total_bond_share = pool_info.total_bond_share.checked_sub(bond_share)?;
    reward_info.bond_share = reward_info.bond_share.checked_sub(bond_share)?;

    reward_info.deposit_amount = reward_info
        .deposit_amount
        .multiply_ratio(user_balance.checked_sub(amount)?, user_balance);

    // update pool info
    POOL_INFO.save(deps.storage, &pool_info)?;

    Ok(Response::new()
        .add_messages(vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.astroport_generator.to_string(),
                funds: vec![],
                msg: to_binary(&AstroportExecuteMsg::Withdraw {
                    lp_token: pool_info.staking_token.to_string(),
                    amount,
                })?,
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: pool_info.staking_token.to_string(),
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

    let config = CONFIG.load(deps.storage)?;
    let reward_info = read_reward_info(deps, env, &config, &staker_addr_validated)?;

    Ok(RewardInfoResponse {
        staker_addr,
        reward_info,
    })
}

fn read_reward_info(
    deps: Deps,
    env: Env,
    config: &Config,
    staker_addr: &Addr,
) -> StdResult<RewardInfoResponseItem> {
    let reward_info = REWARD.load(deps.storage, staker_addr)?;
    let pool_info = POOL_INFO.load(deps.storage)?;

    let has_deposit_amount = !reward_info.deposit_amount.is_zero();

    let lp_balance = query_astroport_pool_balance(
        deps,
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let bond_amount = pool_info.calc_user_balance(lp_balance, reward_info.bond_share);
    Ok(RewardInfoResponseItem {
        asset_token: pool_info.asset_token.to_string(),
        bond_share: reward_info.bond_share,
        bond_amount,
        deposit_amount: if has_deposit_amount {
            Some(reward_info.deposit_amount)
        } else {
            None
        },
        deposit_time: if has_deposit_amount {
            Some(reward_info.deposit_time)
        } else {
            None
        },
    })
}
