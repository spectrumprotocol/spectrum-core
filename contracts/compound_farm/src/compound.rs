use cosmwasm_std::{attr, Attribute, Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128, Decimal, QuerierWrapper, BankMsg};
use kujira::denom::Denom;
use kujira::msg::KujiraMsg;
use kujira::query::KujiraQuery;

use crate::{
    error::ContractError,
    state::CONFIG,
};

use spectrum::compound_farm::CallbackMsg;
use crate::state::{Config, POOL};

fn is_support(
    querier: &QuerierWrapper<KujiraQuery>,
    config: &Config,
    denoms: &[Denom; 2],
    denom: String,
) -> bool {
    denom == denoms[0].to_string()
        || denom == denoms[1].to_string()
        || config.router.query_route(querier, [Denom::from(&denom), denoms[0].clone()]).is_ok()
        || config.router.query_route(querier, [Denom::from(&denom), denoms[1].clone()]).is_ok()
}

/// ## Description
/// Performs compound by sending LP rewards to compound proxy and reinvest received LP token
pub fn compound(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    market_maker: String,
    minimum_receive: Option<Uint128>,
    slippage_tolerance: Option<Decimal>,
) -> Result<Response<KujiraMsg>, ContractError> {

    let config = CONFIG.load(deps.storage)?;

    // Only controller can call this function
    if info.sender != config.controller {
        return Err(ContractError::Unauthorized {});
    }

    let market_maker_addr = deps.api.addr_validate(&market_maker)?;
    let mut pool = POOL.load(deps.storage, &market_maker_addr)?;
    let staked = config.staking.query_stake(
        &deps.querier,
        env.contract.address.clone(),
        pool.market_maker.get_lp_name().into(),
    )?;
    if staked.amount.is_zero() {
        return Ok(Response::default());
    }

    let rewards = config.staking.query_rewards(
        &deps.querier,
        env.contract.address.clone(),
        pool.market_maker.get_lp_name().into(),
    )?;

    let total_fee = config.fee;

    let mut messages: Vec<CosmosMsg<KujiraMsg>> = vec![];
    let mut attributes: Vec<Attribute> = vec![];

    let claim_rewards = config.staking.claim_msg(
        pool.market_maker.get_lp_name().into(),
    )?;
    messages.push(claim_rewards);

    let mut compound_funds: Vec<Coin> = vec![];
    let mut commission_funds: Vec<Coin> = vec![];
    for asset in rewards {
        let pending_reward = pool.rewards.iter_mut().find(|it| it.denom == asset.denom);
        if !is_support(&deps.querier, &config, &pool.denoms, asset.denom.clone()) {
            if let Some(reward) = pending_reward {
                reward.amount += asset.amount;
            } else {
                pool.rewards.push(asset);
            }
            continue;
        }

        let mut reward_amount = asset.amount;
        if let Some(reward) = pending_reward {
            reward_amount += reward.amount;
            reward.amount = Uint128::zero();
        }
        if reward_amount.is_zero() {
            continue;
        }

        let commission_amount = reward_amount * total_fee;
        let compound_amount = reward_amount.checked_sub(commission_amount)?;
        if !compound_amount.is_zero() {
            compound_funds.push(Coin { denom: asset.denom.clone(), amount: compound_amount });
        }

        if !commission_amount.is_zero() {
            commission_funds.push(Coin { denom: asset.denom.clone(), amount: commission_amount });
        }

        attributes.push(attr("token", asset.denom.to_string()));
        attributes.push(attr("compound_amount", compound_amount));
        attributes.push(attr("commission_amount", commission_amount));
    }
    POOL.save(deps.storage, &market_maker_addr, &pool)?;

    if !commission_funds.is_empty() {
        messages.push(BankMsg::Send {
            to_address: config.fee_collector.to_string(),
            amount: commission_funds,
        }.into());
    }

    if !compound_funds.is_empty() {
        let compound = config.compound_proxy.compound_msg(
            market_maker,
            compound_funds,
            None,
            slippage_tolerance)?;
        messages.push(compound);

        let prev_balance = deps.querier.query_balance(&env.contract.address, pool.market_maker.get_lp_name())?;
        messages.push(
            CallbackMsg::Stake {
                prev_balance,
                minimum_receive,
            }
            .to_cosmos_msg(&env.contract.address)?,
        );
    }

    Ok(Response::default()
        .add_messages(messages)
        .add_attribute("action", "compound")
        .add_attributes(attributes))
}

/// ## Description
/// Stakes received LP token to the staking contract.
pub fn stake(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    _info: MessageInfo,
    prev_balance: Coin,
    minimum_receive: Option<Uint128>,
) -> Result<Response<KujiraMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let balance = deps.querier.query_balance(&env.contract.address, &prev_balance.denom)?.amount;
    let amount = balance - prev_balance.amount;

    if let Some(minimum_receive) = minimum_receive {
        if amount < minimum_receive {
            return Err(ContractError::AssertionMinimumReceive {
                minimum_receive,
                amount,
            });
        }
    }

    Ok(Response::default()
        .add_message(
            config.staking.stake_msg(Coin {
                denom: prev_balance.denom.clone(),
                amount,
            }, None)?
        )
        .add_attributes(vec![
            attr("action", "stake"),
            attr("staking_token", prev_balance.denom),
            attr("amount", amount),
        ]))
}
