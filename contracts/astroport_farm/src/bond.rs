use std::collections::HashSet;

use astroport::asset::{Asset, token_asset};
use astroport::querier::query_token_balance;
use cosmwasm_std::{attr, Addr, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, Coin, Decimal};

use crate::error::ContractError;
use crate::state::{ScalingOperation, CONFIG, REWARD, STATE, Config};

use cw20::{Expiration};

use spectrum::adapters::asset::AssetEx;
use spectrum::astroport_farm::{RewardInfoResponse, RewardInfoResponseItem, CallbackMsg};
use spectrum::helper::{ScalingUint128};

/// ## Description
/// Send assets to compound proxy to create LP token and bond received LP token on behalf of sender.
pub fn bond_assets(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    minimum_receive: Option<Uint128>,
    no_swap: Option<bool>,
    slippage_tolerance: Option<Decimal>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let staking_token = config.liquidity_token;

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut funds: Vec<Coin> = vec![];

    let mut uniq = HashSet::new();
    if !assets.clone().into_iter().all(move |x| uniq.insert(x.info.to_string())) {
        return Err(ContractError::DuplicatedAsset {});
    }
    
    for asset in assets.iter() {
        asset.deposit_asset(&info, &env.contract.address, &mut messages)?;
        if !asset.amount.is_zero() {
            if asset.is_native_token() {
                funds.push(Coin {
                    denom: asset.info.to_string(),
                    amount: asset.amount,
                });
            } else {
                messages.push(asset.increase_allowance_msg(
                    config.compound_proxy.0.to_string(),
                    Some(Expiration::AtHeight(env.block.height + 1)),
                )?);
            }
        }
    }

    let compound = config.compound_proxy.compound_msg(assets, funds, no_swap, slippage_tolerance)?;
    messages.push(compound);

    let prev_balance = query_token_balance(&deps.querier, staking_token, &env.contract.address)?;
    messages.push(
        CallbackMsg::BondTo {
            to: info.sender,
            prev_balance,
            minimum_receive,
        }
        .into_cosmos_msg(&env.contract.address)?,
    );

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "bond_assets"))
}

/// ## Description
/// Bond available LP token on the contract on behalf of the user.
pub fn bond_to(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    to: Addr,
    prev_balance: Uint128,
    minimum_receive: Option<Uint128>
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let balance = query_token_balance(&deps.querier, &config.liquidity_token, &env.contract.address)?;
    let amount = balance - prev_balance;

    if let Some(minimum_receive) = minimum_receive {
        if amount < minimum_receive {
            return Err(ContractError::AssertionMinimumReceive {
                minimum_receive,
                amount,
            });
        }
    }

    bond_internal(
        deps,
        env,
        config,
        to,
        amount,
    )
}

/// ## Description
/// Bond received LP token on behalf of the user.
pub fn bond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender_addr: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let staker_addr = deps.api.addr_validate(&sender_addr)?;

    let config = CONFIG.load(deps.storage)?;

    // only staking token contract can execute this message
    if config.liquidity_token != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    bond_internal(
        deps,
        env,
        config,
        staker_addr,
        amount,
    )
}

/// Internal bond function used by bond and bond_to
fn bond_internal(
    deps: DepsMut,
    env: Env,
    config: Config,
    staker_addr: Addr,
    amount: Uint128,
) -> Result<Response, ContractError>{

    let lp_balance = config.staking_contract.query_deposit(
        &deps.querier,
        &config.liquidity_token,
        &env.contract.address,
    )?;

    let mut messages: Vec<CosmosMsg> = vec![];

    let mut state = STATE.load(deps.storage)?;

    // withdraw reward to pending reward; before changing share
    let mut reward_info = REWARD
        .may_load(deps.storage, &staker_addr)?
        .unwrap_or_default();

    // convert amount to share & update
    let bond_share = state.calc_bond_share(amount, lp_balance, ScalingOperation::Truncate);
    state.total_bond_share += bond_share;

    let deposit_amount = state.calc_bond_amount(
        lp_balance + amount,
        bond_share,
    );

    let pool_info = config.pair.query_pool_info(&deps.querier)?;
    reward_info.ensure_deposit_costs(deps.storage)?;
    reward_info.bond(bond_share, deposit_amount, env.block.time.seconds(), &pool_info)?;

    REWARD.save(deps.storage, &staker_addr, &reward_info)?;
    STATE.save(deps.storage, &state)?;

    messages.push(config.staking_contract.deposit_msg(config.liquidity_token.to_string(), amount)?);
    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "bond"),
        attr("amount", amount),
        attr("bond_amount", amount),
    ]))
}

/// ## Description
/// Unbond LP token of sender
pub fn unbond(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {

    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let staker_addr = info.sender;

    let config = CONFIG.load(deps.storage)?;
    let staking_token = config.liquidity_token;

    let lp_balance = config.staking_contract.query_deposit(
        &deps.querier,
        &staking_token,
        &env.contract.address,
    )?;

    let mut state = STATE.load(deps.storage)?;
    let mut reward_info = REWARD.load(deps.storage, &staker_addr)?;

    let user_balance = reward_info.calc_user_balance(
        &state,
        lp_balance,
        env.block.time.seconds(),
    );

    if user_balance < amount {
        return Err(ContractError::UnbondExceedBalance {});
    }

    let bond_share = reward_info.bond_share.multiply_ratio_and_ceil(amount, user_balance);
    state.total_bond_share = state.total_bond_share.checked_sub(bond_share)?;
    reward_info.unbond(bond_share)?;

    // update state
    STATE.save(deps.storage, &state)?;
    REWARD.save(deps.storage, &staker_addr, &reward_info)?;

    Ok(Response::new()
        .add_messages(vec![
            config.staking_contract.withdraw_msg(staking_token.to_string(), amount)?,
            token_asset(staking_token, amount).transfer_msg(&staker_addr)?,
        ])
        .add_attributes(vec![
            attr("action", "unbond"),
            attr("staker_addr", staker_addr),
            attr("amount", amount),
        ]))
}

/// ## Description
/// Returns reward info for the staker.
pub fn query_reward_info(
    deps: Deps,
    env: Env,
    staker_addr: String,
) -> StdResult<RewardInfoResponse> {
    let staker_addr_validated = deps.api.addr_validate(&staker_addr)?;
    let reward_info = read_reward_info(deps, env, &staker_addr_validated)?;

    Ok(RewardInfoResponse {
        staker_addr,
        reward_info,
    })
}

/// Loads reward info from the storage
fn read_reward_info(deps: Deps, env: Env, staker_addr: &Addr) -> StdResult<RewardInfoResponseItem> {
    let mut reward_info = REWARD
        .may_load(deps.storage, staker_addr)?
        .unwrap_or_default();
    let state = STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let staking_token = config.liquidity_token;

    let lp_balance = config.staking_contract.query_deposit(
        &deps.querier,
        &staking_token,
        &env.contract.address,
    )?;

    let bond_amount = reward_info.calc_user_balance(
        &state,
        lp_balance,
        env.block.time.seconds(),
    );
    let total_share = reward_info.bond_share + reward_info.transfer_share;
    reward_info.ensure_deposit_costs(deps.storage)?;
    Ok(RewardInfoResponseItem {
        staking_token: staking_token.to_string(),
        bond_share: reward_info.bond_share,
        bond_amount,
        deposit_amount: if total_share.is_zero() {
            Uint128::zero()
        } else {
            reward_info.deposit_amount
                .multiply_ratio(reward_info.bond_share, total_share)
        },
        deposit_time: reward_info.deposit_time,
        deposit_costs: if total_share.is_zero() {
            vec![]
        } else {
            reward_info.deposit_costs.iter()
                .map(|it| it.multiply_ratio(reward_info.bond_share, total_share))
                .collect()
        }
    })
}
