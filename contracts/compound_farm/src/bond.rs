use cosmwasm_std::{attr, Addr, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128, Coin, Decimal, BankMsg, Order};
use cw_storage_plus::Bound;
use kujira::msg::{DenomMsg, KujiraMsg};
use kujira::querier::KujiraQuerier;
use kujira::query::KujiraQuery;
use spectrum::adapters::kujira::market_maker::MarketMaker;

use crate::error::ContractError;
use crate::state::{ScalingOperation, CONFIG, REWARD, Config, SupplyResponseEx, POOL, PoolInfo, extract_market_maker_from_lp, extract_market_maker_from_clp};

use spectrum::compound_farm::{RewardInfoResponse, RewardInfoResponseItem, CallbackMsg};

/// ## Description
/// Send assets to compound proxy to create LP token and bond received LP token on behalf of sender.
pub fn bond_assets(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    market_maker: String,
    minimum_receive: Option<Uint128>,
    no_swap: Option<bool>,
    slippage_tolerance: Option<Decimal>,
) -> Result<Response<KujiraMsg>, ContractError> {

    let market_maker = MarketMaker(deps.api.addr_validate(&market_maker)?);
    let config = CONFIG.load(deps.storage)?;

    let mut messages: Vec<CosmosMsg<KujiraMsg>> = vec![];

    let compound = config.compound_proxy.compound_msg(
        market_maker.0.to_string(),
        info.funds.clone(),
        no_swap,
        slippage_tolerance
    )?;
    messages.push(compound);

    let prev_balance = market_maker.query_lp_balance(&deps.querier, &env.contract.address)?;
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
    prev_balance: Coin,
    minimum_receive: Option<Uint128>
) -> Result<Response<KujiraMsg>, ContractError> {

    let lp_token = deps.api.addr_validate(&prev_balance.denom)?;
    let config = CONFIG.load(deps.storage)?;

    let balance = &deps.querier.query_balance(&env.contract.address, lp_token)?.amount;
    let amount = balance.checked_sub(prev_balance.amount).or_else(|_|
        Err(ContractError::BalanceLessThanPreviousBalance { })
    )?;

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
        Coin {
            denom: prev_balance.denom,
            amount,
        },
    )
}

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
        [fund] => fund,
        _ => return Err(ContractError::InvalidFunds {}),
    };

    bond_internal(
        deps,
        env,
        config,
        staker_addr,
        fund.clone(),
    )
}

/// Internal bond function used by bond and bond_to
fn bond_internal(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    config: Config,
    staker_addr: Addr,
    fund: Coin,
) -> Result<Response<KujiraMsg>, ContractError>{

    let market_maker_addr = extract_market_maker_from_lp(&fund.denom)?;

    let mut messages: Vec<CosmosMsg<KujiraMsg>> = vec![];
    let pool = match POOL.may_load(deps.storage, &market_maker_addr)? {
        None => {
            let market_maker = MarketMaker(market_maker_addr.clone());
            let mm_config = market_maker.query_config(&deps.querier)?;
            let pool_info = PoolInfo {
                market_maker,
                denoms: mm_config.denoms,
                rewards: vec![],
            };
            POOL.save(deps.storage, &market_maker_addr, &pool_info)?;
            messages.push(DenomMsg::Create {
                subdenom: pool_info.market_maker.0.to_string().into(),
            }.into());
            pool_info
        }
        Some(pool) => pool,
    };

    let lp_balance = config.staking.query_stake(
        &deps.querier,
        env.contract.address.clone(),
        pool.market_maker.get_lp_name().into(),
    )?.amount;

    let clp_supply = KujiraQuerier::new(&deps.querier)
        .query_supply_of(pool.get_clp_name(&env).into())?;

    // withdraw reward to pending reward; before changing share
    let mut reward_info = REWARD
        .may_load(deps.storage, (&staker_addr, &market_maker_addr))?
        .unwrap_or_default();

    // convert amount to share & update
    let bond_share = clp_supply.calc_bond_share(fund.amount, lp_balance, ScalingOperation::Truncate);

    let lp_supply = KujiraQuerier::new(&deps.querier)
        .query_supply_of(pool.market_maker.get_lp_name().into())?;
    let pool_info = pool.market_maker.query_pool(&deps.querier)?;
    reward_info.bond(bond_share, fund.amount, env.block.time.seconds(), &pool_info, lp_supply.amount.amount)?;

    REWARD.save(deps.storage, (&staker_addr, &market_maker_addr), &reward_info)?;

    messages.push(config.staking.stake_msg(
        Coin { denom: pool.market_maker.get_lp_name(), amount: fund.amount },
        None,
    )?);
    messages.push(DenomMsg::Mint {
        recipient: staker_addr,
        denom: pool.get_clp_name(&env).into(),
        amount: bond_share,
    }.into());

    Ok(Response::default()
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "bond"),
            attr("amount", fund.amount),
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
        [fund] => fund,
        _ => return Err(ContractError::InvalidFunds {}),
    };
    let market_maker_addr = extract_market_maker_from_clp(&fund.denom, &env)?;

    let staker_addr = info.sender;
    let config = CONFIG.load(deps.storage)?;
    let pool = POOL.load(deps.storage, &market_maker_addr)?;

    let lp_balance = config.staking.query_stake(
        &deps.querier,
        env.contract.address.clone(),
        pool.market_maker.get_lp_name().into(),
    )?.amount;

    let clp_supply = KujiraQuerier::new(&deps.querier)
        .query_supply_of(pool.get_clp_name(&env).into())?;
    let mut reward_info = REWARD.load(deps.storage, (&staker_addr, &market_maker_addr))?;

    let amount = clp_supply.calc_bond_amount(lp_balance, fund.amount);
    let amount = reward_info.limit_user_lp(
        amount,
        env.block.time.seconds(),
    );

    reward_info.unbond(fund.amount).or_else(|_|
        Err(ContractError::UnbondExceedBalance {  })
    )?;

    // update state
    REWARD.save(deps.storage, (&staker_addr, &market_maker_addr), &reward_info)?;

    let mut messages: Vec<CosmosMsg<KujiraMsg>> = vec![];
    messages.push(DenomMsg::Burn {
        denom: pool.get_clp_name(&env).into(),
        amount: fund.amount
    }.into());
    let withdraw_coin = Coin {
        denom: pool.market_maker.get_lp_name(),
        amount
    };
    messages.push(
        config.staking.withdraw_msg(withdraw_coin.clone())?
    );
    messages.push(BankMsg::Send {
        to_address: staker_addr.to_string(),
        amount: vec![withdraw_coin]
    }.into());

    Ok(Response::default()
        .add_messages(messages)
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
    limit: Option<u8>,
    start_after: Option<String>,
) -> StdResult<RewardInfoResponse> {
    let validated_addr = deps.api.addr_validate(&staker_addr)?;
    let reward_infos = read_reward_infos(deps, env, &validated_addr, limit, start_after)?;

    Ok(RewardInfoResponse {
        staker_addr,
        reward_infos,
    })
}

const DEFAULT_LIMIT: u8 = 50;
const MAX_LIMIT: u8 = 50;
/// Loads reward info from the storage
fn read_reward_infos(
    deps: Deps<KujiraQuery>,
    env: Env,
    staker_addr: &Addr,
    limit: Option<u8>,
    start_after: Option<String>,
) -> StdResult<Vec<RewardInfoResponseItem>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into()));

    let config = CONFIG.load(deps.storage)?;
    REWARD.prefix(staker_addr)
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|it| {
            let (market_maker, reward_info) = it?;

            let pool = POOL.load(deps.storage, &market_maker)?;
            let clp_supply = KujiraQuerier::new(&deps.querier)
                .query_supply_of(pool.get_clp_name(&env).into())?;
            let lp_balance = config.staking.query_stake(
                &deps.querier,
                env.contract.address.clone(),
                pool.market_maker.get_lp_name().into()
            )?.amount;
            let bond_share = deps.querier.query_balance(staker_addr, pool.get_clp_name(&env))?.amount;
            let bond_amount = clp_supply.calc_bond_amount(lp_balance, bond_share);
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
                market_maker,
                staking_token: pool.market_maker.get_lp_name(),
                bond_share,
                bond_amount,
                deposit_amount: if total_share.is_zero() {
                    Uint128::zero()
                } else {
                    reward_info.deposit_amount
                        .multiply_ratio(bond_share, total_share)
                },
                deposit_time: reward_info.deposit_time,
                deposit_costs: if total_share.is_zero() {
                    [Uint128::zero(), Uint128::zero()]
                } else {
                    [
                        reward_info.deposit_costs[0].multiply_ratio(bond_share, total_share),
                        reward_info.deposit_costs[1].multiply_ratio(bond_share, total_share),
                    ]
                }
            })
        })
        .collect::<StdResult<_>>()
}
