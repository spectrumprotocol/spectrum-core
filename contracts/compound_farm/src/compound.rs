use cosmwasm_std::{attr, Attribute, Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128, Decimal, QuerierWrapper, BankMsg};
use kujira::denom::Denom;
use kujira::msg::KujiraMsg;
use kujira::query::KujiraQuery;
use spectrum::adapters::kujira::market_maker::{ConfigResponse};

use crate::{
    error::ContractError,
    state::CONFIG,
};

use spectrum::compound_farm::CallbackMsg;
use crate::state::Config;


fn is_support(
    querier: &QuerierWrapper<KujiraQuery>,
    config: &Config,
    mm_config: &ConfigResponse,
    denom: String,
) -> bool {
    denom == mm_config.denoms[0].to_string()
        || denom == mm_config.denoms[1].to_string()
        || config.router.query_route(querier, [Denom::from(&denom), mm_config.denoms[0].clone()]).is_ok()
        || config.router.query_route(querier, [Denom::from(&denom), mm_config.denoms[1].clone()]).is_ok()
}

/// ## Description
/// Performs compound by sending LP rewards to compound proxy and reinvest received LP token
pub fn compound(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    minimum_receive: Option<Uint128>,
    slippage_tolerance: Option<Decimal>,
) -> Result<Response<KujiraMsg>, ContractError> {

    let config = CONFIG.load(deps.storage)?;

    // Only controller can call this function
    if info.sender != config.controller {
        return Err(ContractError::Unauthorized {});
    }

    let staked = config.staking.query_stake(
        &deps.querier,
        env.contract.address.clone(),
        config.market_maker.get_lp_name().into(),
    )?;

    let total_fee = config.fee;

    let mut messages: Vec<CosmosMsg<KujiraMsg>> = vec![];
    let mut attributes: Vec<Attribute> = vec![];

    let rewards = staked.fills;

    let claim_rewards = config.staking.claim_msg(
        config.market_maker.get_lp_name().into(),
    )?;
    messages.push(claim_rewards);

    let mut compound_funds: Vec<Coin> = vec![];
    let mut commission_funds: Vec<Coin> = vec![];
    let mm_config = config.market_maker.query_config(&deps.querier)?;
    for asset in rewards {
        let reward_amount = asset.amount;
        if reward_amount.is_zero() || staked.amount.is_zero() {
            continue;
        }
        if !is_support(&deps.querier, &config, &mm_config, asset.denom.clone()) {
            // TODO: save uncompound rewards
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

    if !commission_funds.is_empty() {
        messages.push(BankMsg::Send {
            to_address: config.fee_collector.to_string(),
            amount: commission_funds,
        }.into());
    }

    if !compound_funds.is_empty() {
        let compound = config.compound_proxy.compound_msg(
            config.market_maker.0.to_string(),
            compound_funds,
            None,
            slippage_tolerance)?;
        messages.push(compound);

        let prev_balance = deps.querier.query_balance(&env.contract.address, config.market_maker.get_lp_name())?.amount;
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
    prev_balance: Uint128,
    minimum_receive: Option<Uint128>,
) -> Result<Response<KujiraMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let staking_token = config.market_maker.get_lp_name();
    let balance = deps.querier.query_balance(&env.contract.address, &staking_token)?.amount;
    let amount = balance - prev_balance;

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
                denom: staking_token.clone(),
                amount,
            }, None)?
        )
        .add_attributes(vec![
            attr("action", "stake"),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}
