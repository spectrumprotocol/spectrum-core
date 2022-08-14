use std::cmp;
use std::collections::HashMap;
use cosmwasm_std::{Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, QuerierWrapper, Response, StdResult, Uint128};
use astroport::asset::{addr_validate_to_lower, Asset, AssetInfo, token_asset};
use astroport::querier::query_token_balance;
use crate::error::ContractError;
use astroport::generator::{PendingTokenResponse, UserInfoV2};
use astroport::restricted_vector::RestrictedVector;
use spectrum::adapters::asset::AssetEx;
use crate::astro_generator::GeneratorEx;
use crate::model::{CallbackMsg, Config, PoolInfo, RewardInfo, UserInfo};
use crate::state::{CONFIG, POOL_CONFIG, POOL_INFO, REWARD_INFO, USER_INFO};

pub fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    staker_addr: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {

    // reward cannot be claimed if there is no record
    let mut messages: Vec<CosmosMsg> = vec![];
    if POOL_INFO.has(deps.storage, &info.sender) {
        let config = CONFIG.load(deps.storage)?;
        messages.push(config.generator.claim_rewards_msg(vec![info.sender.to_string()])?);
        messages.push(CallbackMsg::AfterClaimed {
            lp_token: info.sender.clone(),
        }.to_cosmos_msg(&env.contract.address)?);
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_message(CallbackMsg::Deposit {
            lp_token: info.sender,
            staker_addr,
            amount,
        }.to_cosmos_msg(&env.contract.address)?)
    )
}

pub fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lp_token: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let config = CONFIG.load(deps.storage)?;
    Ok(Response::new()
        .add_message(config.generator.claim_rewards_msg(vec![lp_token.to_string()])?)
        .add_message(CallbackMsg::AfterClaimed {
            lp_token: lp_token.clone(),
        }.to_cosmos_msg(&env.contract.address)?)
        .add_message(CallbackMsg::Withdraw {
            lp_token,
            staker_addr: info.sender,
            amount,
        }.to_cosmos_msg(&env.contract.address)?)
    )
}

pub fn execute_claim_rewards(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lp_tokens: Vec<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let mut messages: Vec<CosmosMsg> = vec![
        config.generator.claim_rewards_msg(lp_tokens.clone())?
    ];
    for lp_token in lp_tokens {
        let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
        messages.push(CallbackMsg::AfterClaimed {
            lp_token: lp_token.clone(),
        }.to_cosmos_msg(&env.contract.address)?);
        messages.push(CallbackMsg::ClaimRewards {
            lp_token,
            staker_addr: info.sender.clone(),
        }.to_cosmos_msg(&env.contract.address)?);
    }

    Ok(Response::new()
        .add_messages(messages)
    )
}

fn reconcile_astro_to_pool_info(
    querier: &QuerierWrapper,
    contract_addr: &Addr,
    config: &Config,
    astro_user_info: &UserInfoV2,
    add_pending_amount: Uint128,
    pool_info: &mut PoolInfo,
    astro_reward: &mut RewardInfo,
) -> StdResult<()> {
    let astro_amount = query_token_balance(querier, config.astro_token.clone(), contract_addr.clone())?;
    let add_astro_amount = astro_amount.saturating_sub(astro_reward.reconciled_amount);
    let target_add_astro_amount = (astro_user_info.reward_user_index - pool_info.prev_reward_user_index) * astro_user_info.virtual_amount;
    let earned_astro_amount = cmp::min(add_astro_amount, target_add_astro_amount) + add_pending_amount;
    let fee = earned_astro_amount * config.fee_rate;
    let net_astro_amount = earned_astro_amount - fee;
    let based_astro = net_astro_amount.multiply_ratio(
        astro_user_info.amount * Decimal::percent(40),
        astro_user_info.virtual_amount,
    );
    let boosted_astro = net_astro_amount.checked_sub(based_astro)?;
    let to_staker = boosted_astro * config.staker_rate;
    let to_lp = boosted_astro - to_staker + based_astro;
    let astro_per_share = Decimal::from_ratio(to_lp, pool_info.total_bond_share);
    astro_reward.fee += fee;
    astro_reward.staker_income += to_staker;
    astro_reward.reconciled_amount += earned_astro_amount;
    pool_info.prev_reward_user_index = astro_user_info.reward_user_index;
    pool_info.reward_indexes.update(&config.astro_token, astro_per_share)?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn reconcile_token_to_pool_info(
    querier: &QuerierWrapper,
    token: &Addr,
    contract_addr: &Addr,
    config: &Config,
    target_add_token_amount: Uint128,
    add_pending_amount: Uint128,
    pool_info: &mut PoolInfo,
    token_reward: &mut RewardInfo,
) -> StdResult<()> {
    let token_amount = query_token_balance(querier, token.clone(), contract_addr.clone())?;
    let add_token_amount = token_amount.saturating_sub(token_reward.reconciled_amount);
    let earned_token_amount = cmp::min(add_token_amount, target_add_token_amount) + add_pending_amount;
    let fee = earned_token_amount * config.fee_rate;
    let net_token_amount = earned_token_amount - fee;
    let token_per_share = Decimal::from_ratio(net_token_amount, pool_info.total_bond_share);
    token_reward.fee += fee;
    token_reward.reconciled_amount += earned_token_amount;
    pool_info.reward_indexes.update(token, token_per_share)?;

    Ok(())
}

pub fn callback_after_claimed(
    deps: DepsMut,
    env: Env,
    lp_token: Addr,
) -> Result<Response, ContractError> {

    // load
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = POOL_INFO.load(deps.storage, &lp_token)?;
    let astro_user_info = config.generator.query_user_info(&deps.querier, &lp_token, &env.contract.address)?;

    // reconcile astro
    let mut astro_reward = REWARD_INFO.may_load(deps.storage, &config.astro_token)?
        .unwrap_or_default();
    reconcile_astro_to_pool_info(
        &deps.querier,
        &env.contract.address,
        &config,
        &astro_user_info,
        Uint128::zero(),
        &mut pool_info,
        &mut astro_reward,
    )?;
    REWARD_INFO.save(deps.storage, &config.astro_token, &astro_reward)?;

    // reconcile other tokens
    let rewards_debt_map: HashMap<_, _> =
        pool_info.prev_reward_debt_proxy.inner_ref().iter().cloned().collect();
    for (token, debt) in astro_user_info.reward_debt_proxy.inner_ref() {
        let mut token_reward = REWARD_INFO.may_load(deps.storage, token)?
            .unwrap_or_default();
        let prev_debt = rewards_debt_map.get(token).cloned().unwrap_or_default();
        let target_add_token_amount = debt.saturating_sub(prev_debt);
        reconcile_token_to_pool_info(
            &deps.querier,
            token,
            &env.contract.address,
            &config,
            target_add_token_amount,
            Uint128::zero(),
            &mut pool_info,
            &mut token_reward,
        )?;
        REWARD_INFO.save(deps.storage, token, &token_reward)?;
    }
    pool_info.prev_reward_debt_proxy = astro_user_info.reward_debt_proxy;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    Ok(Response::new())
}

pub fn callback_after_bond_changed(
    deps: DepsMut,
    env: Env,
    lp_token: Addr,
    prev_assets: Vec<Asset>,
) -> Result<Response, ContractError> {

    // load
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = POOL_INFO.load(deps.storage, &lp_token)?;

    // debt will reset after share changed
    let astro_user_info = config.generator.query_user_info(&deps.querier, &lp_token, &env.contract.address)?;
    pool_info.prev_reward_user_index = astro_user_info.reward_user_index;
    pool_info.prev_reward_debt_proxy = astro_user_info.reward_debt_proxy;

    // asset rewards
    for prev_asset in prev_assets {
        let balance = prev_asset.info.query_pool(&deps.querier, env.contract.address.clone())?;
        let add_amount = balance.checked_sub(prev_asset.amount)?;
        if !add_amount.is_zero() {
            let token = Addr::unchecked(prev_asset.info.to_string());
            let mut token_reward = REWARD_INFO.may_load(deps.storage, &token)?
                .unwrap_or_default();
            let fee = add_amount * config.fee_rate;
            let net_amount = add_amount - fee;
            let token_per_share = Decimal::from_ratio(net_amount, pool_info.total_bond_share);
            token_reward.fee += fee;
            token_reward.reconciled_amount += add_amount;
            pool_info.reward_indexes.update(&token, token_per_share)?;
            REWARD_INFO.save(deps.storage, &token, &token_reward)?;
        }
    }

    // save
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    Ok(Response::default())
}

fn reconcile_to_user_info(
    pool_info: &PoolInfo,
    user_info: &mut UserInfo,
) -> StdResult<()> {
    let user_indexes: HashMap<_, _> =
        user_info.reward_indexes.inner_ref().iter().cloned().collect();
    for (token, index) in pool_info.reward_indexes.inner_ref() {
        let user_index = user_indexes.get(token).cloned().unwrap_or_default();
        let amount = (*index - user_index) * user_info.bond_share;
        user_info.pending_rewards.update(token, amount)?;
    }
    user_info.reward_indexes = pool_info.reward_indexes.clone();

    Ok(())
}

pub fn callback_deposit(
    deps: DepsMut,
    env: Env,
    lp_token: Addr,
    staker_addr: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {

    // load
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = POOL_INFO.may_load(deps.storage, &lp_token)?
        .unwrap_or_default();
    let mut user_info = USER_INFO.may_load(deps.storage, (&lp_token, &staker_addr))?
        .unwrap_or_else(|| UserInfo::create(&pool_info));

    // update
    reconcile_to_user_info(&pool_info, &mut user_info)?;
    let total_bond_amount = config.generator.query_deposit(&deps.querier, &lp_token, &env.contract.address)?;
    let share = pool_info.calc_bond_share(total_bond_amount, amount, false);
    user_info.bond_share += share;
    pool_info.total_bond_share += share;

    // save
    USER_INFO.save(deps.storage, (&lp_token, &staker_addr), &user_info)?;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    // fetch prev asset balance
    let mut prev_assets: Vec<Asset> = vec![];
    if let Some(pool_config) = POOL_CONFIG.may_load(deps.storage, &lp_token)? {
        for asset_reward in pool_config.asset_rewards {
            let amount = asset_reward.query_pool(&deps.querier, env.contract.address.clone())?;
            prev_assets.push(Asset {
                info: asset_reward,
                amount,
            });
        }
    }

    let deposit_msg = config.generator.deposit_msg(lp_token.to_string(), amount)?;
    Ok(Response::new()
        .add_message(deposit_msg)
        .add_message(CallbackMsg::AfterBondChanged {
            lp_token,
            prev_assets,
        }.to_cosmos_msg(&env.contract.address)?)
        .add_attribute("add_share", share)
    )
}

pub fn callback_withdraw(
    deps: DepsMut,
    env: Env,
    lp_token: Addr,
    staker_addr: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {

    // load
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = POOL_INFO.load(deps.storage, &lp_token)?;
    let mut user_info = USER_INFO.load(deps.storage, (&lp_token, &staker_addr))?;

    // update
    reconcile_to_user_info(&pool_info, &mut user_info)?;
    let total_bond_amount = config.generator.query_deposit(&deps.querier, &lp_token, &env.contract.address)?;
    let share = pool_info.calc_bond_share(total_bond_amount, amount, true);
    user_info.bond_share -= share;
    pool_info.total_bond_share -= share;

    // save
    USER_INFO.save(deps.storage, (&lp_token, &staker_addr), &user_info)?;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    // fetch prev asset balance
    let mut prev_assets: Vec<Asset> = vec![];
    if let Some(pool_config) = POOL_CONFIG.may_load(deps.storage, &lp_token)? {
        for asset_reward in pool_config.asset_rewards {
            let amount = asset_reward.query_pool(&deps.querier, env.contract.address.clone())?;
            prev_assets.push(Asset {
                info: asset_reward,
                amount,
            });
        }
    }

    let withdraw_msg = config.generator.withdraw_msg(lp_token.to_string(), amount)?;
    Ok(Response::new()
        .add_message(withdraw_msg)
        .add_message(CallbackMsg::AfterBondChanged {
            lp_token,
            prev_assets,
        }.to_cosmos_msg(&env.contract.address)?)
        .add_attribute("deduct_share", share)
    )
}

pub fn callback_claim_rewards(
    deps: DepsMut,
    _env: Env,
    lp_token: Addr,
    staker_addr: Addr,
) -> Result<Response, ContractError> {

    // load
    let mut user_info = USER_INFO.load(deps.storage, (&lp_token, &staker_addr))?;

    // send
    let mut messages: Vec<CosmosMsg> = vec![];
    for (token, amount) in user_info.pending_rewards.inner_ref() {
        if amount.is_zero() {
            continue;
        }

        let mut reward_info = REWARD_INFO.load(deps.storage, token)?;
        reward_info.reconciled_amount -= amount;
        REWARD_INFO.save(deps.storage, token, &reward_info)?;

        let asset = token_asset(token.clone(), *amount);
        messages.push(asset.transfer_msg(&staker_addr)?);
    }
    user_info.pending_rewards = RestrictedVector::default();

    // save
    USER_INFO.save(deps.storage, (&lp_token, &staker_addr), &user_info)?;

    Ok(Response::new()
        .add_messages(messages)
    )
}

pub fn query_pending_token(
    deps: Deps,
    env: Env,
    lp_token: String,
    user: String,
) -> Result<PendingTokenResponse, ContractError> {

    // load
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let user = addr_validate_to_lower(deps.api, &user)?;
    let config = CONFIG.load(deps.storage)?;
    let mut pool_info = POOL_INFO.load(deps.storage, &lp_token)?;
    let mut user_info = USER_INFO.may_load(deps.storage, (&lp_token, &user))?
        .unwrap_or_else(|| UserInfo::create(&pool_info));
    let astro_user_info = config.generator.query_user_info(&deps.querier, &lp_token, &env.contract.address)?;
    let pending_token = config.generator.query_pending_token(&deps.querier, &lp_token, &env.contract.address)?;

    // reconcile astro
    let mut astro_reward = REWARD_INFO.may_load(deps.storage, &config.astro_token)?
        .unwrap_or_default();
    reconcile_astro_to_pool_info(
        &deps.querier,
        &env.contract.address,
        &config,
        &astro_user_info,
        pending_token.pending,
        &mut pool_info,
        &mut astro_reward,
    )?;

    // reconcile other tokens
    let rewards_debt_map: HashMap<_, _> =
        pool_info.prev_reward_debt_proxy.inner_ref().iter().cloned().collect();
    let pending_token_map: HashMap<_, _> = if let Some(tokens) = pending_token.pending_on_proxy {
        tokens.into_iter().map(|it| (it.info.to_string(), it.amount)).collect()
    } else {
        HashMap::new()
    };
    for (token, debt) in astro_user_info.reward_debt_proxy.inner_ref() {
        let mut token_reward = REWARD_INFO.may_load(deps.storage, token)?
            .unwrap_or_default();
        let prev_debt = rewards_debt_map.get(token).cloned().unwrap_or_default();
        let target_add_token_amount = debt.saturating_sub(prev_debt);
        let add_pending_amount = pending_token_map.get(&token.to_string()).cloned().unwrap_or_default();
        reconcile_token_to_pool_info(
            &deps.querier,
            token,
            &env.contract.address,
            &config,
            target_add_token_amount,
            add_pending_amount,
            &mut pool_info,
            &mut token_reward,
        )?;
    }
    pool_info.prev_reward_debt_proxy = astro_user_info.reward_debt_proxy;

    // reconcile to user info
    reconcile_to_user_info(&pool_info, &mut user_info)?;

    // build data
    let mut pending = Uint128::zero();
    let mut pending_on_proxy: Vec<Asset> = vec![];
    for (addr, amount) in user_info.pending_rewards.inner_ref() {
        if addr == &config.astro_token {
            pending = *amount;
        } else {
            let info = if addr.to_string().starts_with('u') {
                AssetInfo::NativeToken { denom: addr.to_string() }
            } else {
                AssetInfo::Token { contract_addr: addr.clone() }
            };
            pending_on_proxy.push(Asset { info, amount: *amount });
        }
    }

    Ok(PendingTokenResponse {
        pending,
        pending_on_proxy: if pending_on_proxy.is_empty() {
            None
        } else {
            Some(pending_on_proxy)
        },
    })
}

pub fn query_deposit(
    deps: Deps,
    env: Env,
    lp_token: String,
    user: String,
) -> Result<Uint128, ContractError> {

    // load
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let user = addr_validate_to_lower(deps.api, &user)?;
    let config = CONFIG.load(deps.storage)?;
    let pool_info = POOL_INFO.load(deps.storage, &lp_token)?;
    let user_info = USER_INFO.may_load(deps.storage, (&lp_token, &user))?
        .unwrap_or_else(|| UserInfo::create(&pool_info));

    // query
    let total_bond_amount = config.generator.query_deposit(&deps.querier, &lp_token, &env.contract.address)?;
    let user_bond_amount = pool_info.calc_bond_amount(total_bond_amount, user_info.bond_share);
    Ok(user_bond_amount)
}
