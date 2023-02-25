use cosmwasm_std::{attr, Addr, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, Coin, Decimal, BankMsg};
use kujira::msg::{DenomMsg, KujiraMsg};
use kujira::querier::KujiraQuerier;
use kujira::query::KujiraQuery;

use crate::error::ContractError;
use crate::state::{ScalingOperation, CONFIG, REWARD, Config, SupplyResponseEx};

use spectrum::compound_farm::{RewardInfoResponse, RewardInfoResponseItem, CallbackMsg};

/// ## Description
/// Send assets to compound proxy to create LP token and bond received LP token on behalf of sender.
pub fn bond_assets(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    minimum_receive: Option<Uint128>,
    no_swap: Option<bool>,
    slippage_tolerance: Option<Decimal>,
) -> Result<Response<KujiraMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut messages: Vec<CosmosMsg<KujiraMsg>> = vec![];

    let compound = config.compound_proxy.compound_msg(
        config.market_maker.0.to_string(),
        info.funds.clone(),
        no_swap,
        slippage_tolerance
    )?;
    messages.push(compound);

    let prev_balance = config.market_maker.query_lp_balance(&deps.querier, &env.contract.address)?.amount;
    messages.push(
        CallbackMsg::BondTo {
            to: info.sender,
            prev_balance,
            minimum_receive,
        }
        .to_cosmos_msg(&env.contract.address)?,
    );

    Ok(Response::default()
        .add_messages(messages)
        .add_attribute("action", "bond_assets"))
}

/// ## Description
/// Bond available LP token on the contract on behalf of the user.
pub fn bond_to(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    _info: MessageInfo,
    to: Addr,
    prev_balance: Uint128,
    minimum_receive: Option<Uint128>
) -> Result<Response<KujiraMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let balance = config.market_maker.query_lp_balance(&deps.querier, &env.contract.address)?.amount;
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

fn clp_name(env: &Env) -> String { format!("factory/{0}/clp", env.contract.address) }

/// ## Description
/// Bond received LP token on behalf of the user.
pub fn bond(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    sender_addr: Option<String>,
) -> Result<Response<KujiraMsg>, ContractError> {
    let staker_addr = match sender_addr {
        None => info.sender.clone(),
        Some(sender_addr) => deps.api.addr_validate(&sender_addr)?,
    };

    let config = CONFIG.load(deps.storage)?;

    let fund = match &info.funds[..] {
        [fund] if fund.denom == config.market_maker.get_lp_name() => fund,
        _ => return Err(ContractError::InvalidFunds {}),
    };

    bond_internal(
        deps,
        env,
        config,
        staker_addr,
        fund.amount,
    )
}

/// Internal bond function used by bond and bond_to
fn bond_internal(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    config: Config,
    staker_addr: Addr,
    amount: Uint128,
) -> Result<Response<KujiraMsg>, ContractError>{

    let lp_balance = config.staking.query_stake(
        &deps.querier,
        env.contract.address.clone(),
        config.market_maker.get_lp_name().into(),
    )?.amount;

    let clp_supply = KujiraQuerier::new(&deps.querier)
        .query_supply_of(clp_name(&env).into())?;

    // withdraw reward to pending reward; before changing share
    let mut reward_info = REWARD
        .may_load(deps.storage, &staker_addr)?
        .unwrap_or_default();

    // convert amount to share & update
    let bond_share = clp_supply.calc_bond_share(amount, lp_balance, ScalingOperation::Truncate);

    let lp_supply = KujiraQuerier::new(&deps.querier)
        .query_supply_of(config.market_maker.get_lp_name().into())?;
    let pool_info = config.market_maker.query_pool(&deps.querier)?;
    reward_info.bond(bond_share, amount, env.block.time.seconds(), &pool_info, lp_supply.amount.amount)?;

    REWARD.save(deps.storage, &staker_addr, &reward_info)?;

    let stake_msg = config.staking.stake_msg::<KujiraMsg>(
        Coin { denom: config.market_maker.get_lp_name(), amount },
        None,
    )?;
    let mint_msg = DenomMsg::Mint {
        recipient: staker_addr,
        denom: clp_name(&env).into(),
        amount: bond_share,
    };

    Ok(Response::default()
        .add_message(stake_msg)
        .add_message(mint_msg)
        .add_attributes(vec![
            attr("action", "bond"),
            attr("amount", amount),
        ]))
}

/// ## Description
/// Unbond LP token of sender
pub fn unbond(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
) -> Result<Response<KujiraMsg>, ContractError> {

    let fund = match &info.funds[..] {
        [fund] if fund.denom == clp_name(&env) => fund,
        _ => return Err(ContractError::InvalidFunds {}),
    };

    let staker_addr = info.sender;
    let config = CONFIG.load(deps.storage)?;

    let lp_balance = config.staking.query_stake(
        &deps.querier,
        env.contract.address.clone(),
        config.market_maker.get_lp_name().into(),
    )?.amount;

    let clp_supply = KujiraQuerier::new(&deps.querier)
        .query_supply_of(clp_name(&env).into())?;
    let mut reward_info = REWARD.load(deps.storage, &staker_addr)?;

    let amount = clp_supply.calc_bond_amount(lp_balance, fund.amount);
    let amount = reward_info.limit_user_lp(
        amount,
        env.block.time.seconds(),
    );

    reward_info.unbond(fund.amount)?;

    // update state
    REWARD.save(deps.storage, &staker_addr, &reward_info)?;

    let burn_msg = DenomMsg::Burn {
        denom: clp_name(&env).into(),
        amount: fund.amount
    };
    let withdraw_coin = Coin {
        denom: config.market_maker.get_lp_name(),
        amount
    };
    let withdraw_msg = config.staking.withdraw_msg::<KujiraMsg>(withdraw_coin.clone())?;
    let transfer_msg = BankMsg::Send {
        to_address: staker_addr.to_string(),
        amount: vec![withdraw_coin]
    };

    Ok(Response::default()
        .add_message(burn_msg)
        .add_message(withdraw_msg)
        .add_message(transfer_msg)
        .add_attributes(vec![
            attr("action", "unbond"),
            attr("staker_addr", staker_addr),
            attr("amount", amount),
        ]))
}

/// ## Description
/// Returns reward info for the staker.
pub fn query_reward_info(
    deps: Deps<KujiraQuery>,
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
fn read_reward_info(deps: Deps<KujiraQuery>, env: Env, staker_addr: &Addr) -> StdResult<RewardInfoResponseItem> {
    let reward_info = REWARD
        .may_load(deps.storage, staker_addr)?
        .unwrap_or_default();
    let state = KujiraQuerier::new(&deps.querier)
        .query_supply_of(clp_name(&env).into())?;
    let config = CONFIG.load(deps.storage)?;

    let lp_balance = config.staking.query_stake(
        &deps.querier,
        env.contract.address.clone(),
        config.market_maker.get_lp_name().into()
    )?.amount;

    let bond_share = deps.querier.query_balance(staker_addr, clp_name(&env))?.amount;
    let bond_amount = state.calc_bond_amount(lp_balance, bond_share);
    let bond_amount = reward_info.limit_user_lp(
        bond_amount,
        env.block.time.seconds(),
    );
    let total_share = if reward_info.deposit_share < bond_share {
        bond_share
    } else {
        reward_info.deposit_share
    };
    Ok(RewardInfoResponseItem {
        staking_token: config.market_maker.get_lp_name(),
        bond_share,
        bond_amount,
        deposit_amount: if total_share.is_zero() {
            Uint128::zero()
        } else {
            reward_info.deposit_amount
                .multiply_ratio(bond_share, total_share)
        },
        deposit_time: reward_info.deposit_time,
        deposit_costs: [
            reward_info.deposit_costs[0].multiply_ratio(bond_share, total_share),
            reward_info.deposit_costs[1].multiply_ratio(bond_share, total_share),
        ]
    })
}