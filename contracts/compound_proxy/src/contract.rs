use std::cmp::min;
use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use std::convert::TryInto;

use cosmwasm_std::{entry_point, to_binary, Binary, Coin, CosmosMsg, Decimal, Decimal256, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128, Uint256, Empty};
use kujira::asset::{Asset, AssetInfo};
use kujira::denom::Denom;
use kujira::query::{KujiraQuery};
use spectrum::adapters::kujira::market_maker::MarketMaker;
use spectrum::adapters::pair::Pair;
use spectrum::compound_proxy::{CallbackMsg, CompoundSimulationResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use spectrum::router::Router;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let router = Router(deps.api.addr_validate(&msg.router)?);

    let config = Config {
        router,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

/// ## Description
/// Exposes execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Compound {
            market_maker,
            no_swap,
            slippage_tolerance,
        } => compound(
                deps,
                env,
                info,
                market_maker,
                no_swap.unwrap_or(false),
                slippage_tolerance,
            ),
        ExecuteMsg::Callback(msg) => handle_callback(deps, env, info, msg),
    }
}

/// ## Description
/// Performs rewards compounding to LP token. Sender must do token approval upon calling this function.
#[allow(clippy::too_many_arguments)]
fn compound(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    market_maker: String,
    no_swap: bool,
    slippage_tolerance: Option<Decimal>,
) -> Result<Response, ContractError> {

    let market_maker = MarketMaker(deps.api.addr_validate(&market_maker)?);
    let config = CONFIG.load(deps.storage)?;
    let mm_config = market_maker.query_config(&deps.querier)?;
    let [token_x, token_y] = mm_config.denoms;

    let mut messages: Vec<CosmosMsg> = vec![];

    // Swap reward to asset in the pair
    let mut coin_x = deps.querier.query_balance(&env.contract.address, token_x.to_string())?;
    let mut coin_y = deps.querier.query_balance(&env.contract.address, token_y.to_string())?;
    for fund in info.funds.iter() {
        if fund.denom == token_x.to_string() {
            coin_x.amount -= fund.amount;
        } else if fund.denom == token_y.to_string() {
            coin_y.amount -= fund.amount;
        } else if let Ok(swap_msg) = config.router.try_build_swap_msg(&deps.querier, Denom::from(&fund.denom), token_x.clone(), fund.amount) {
            messages.push(swap_msg);
        } else if let Ok(swap_msg) = config.router.try_build_swap_msg(&deps.querier, Denom::from(&fund.denom), token_y.clone(), fund.amount) {
            messages.push(swap_msg);
        } else {
            return Err(ContractError::InvalidAsset {})
        }
    }

    if !no_swap {
        messages.push(CallbackMsg::OptimalSwap {
            pair: Pair(mm_config.fin_contract),
            market_maker: MarketMaker(market_maker.0.clone()),
            prev_balances: [coin_x.clone(), coin_y.clone()],
            slippage_tolerance: slippage_tolerance.map(Decimal256::from),
        }.into_cosmos_msg(&env.contract.address)?);
    }

    messages.push(
        CallbackMsg::ProvideLiquidity {
            market_maker,
            prev_balances: [coin_x, coin_y],
            slippage_tolerance,
        }
        .into_cosmos_msg(&env.contract.address)?,
    );

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "compound"))
}

/// # Description
/// Handle the callbacks describes in the [`CallbackMsg`]. Returns an [`ContractError`] on failure, otherwise returns the [`Response`]
fn handle_callback(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Callback functions can only be called by this contract itself
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    match msg {
        CallbackMsg::OptimalSwap {
            pair,
            market_maker,
            prev_balances ,
            slippage_tolerance,
        } => optimal_swap(deps, env, info, pair, market_maker, prev_balances, slippage_tolerance),
        CallbackMsg::ProvideLiquidity {
            market_maker,
            prev_balances,
            slippage_tolerance,
        } => provide_liquidity(deps, env, info, market_maker, prev_balances, slippage_tolerance),
    }
}

/// # Description
/// Performs optimal swap of assets in the pair contract.
fn optimal_swap(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    _info: MessageInfo,
    pair: Pair,
    market_maker: MarketMaker,
    [prev_x, prev_y]: [Coin; 2],
    slippage_tolerance: Option<Decimal256>,
) -> Result<Response, ContractError> {
    let mut messages: Vec<CosmosMsg> = vec![];
    let amount_x = deps.querier.query_balance(&env.contract.address, &prev_x.denom)?.amount - prev_x.amount;
    let amount_y = deps.querier.query_balance(&env.contract.address, &prev_y.denom)?.amount - prev_y.amount;

    let pool = market_maker.query_pool(&deps.querier)?;
    let [pool_x, pool_y] = pool.balances;

    if !pool_x.is_zero() && !pool_y.is_zero() {
        let (amount, invert) = calculate_optimal_swap(amount_x, amount_y, pool_x, pool_y);
        if !amount.is_zero() {
            let swap_msg = pair.swap_msg(
                Coin { denom: if invert { prev_y.denom } else { prev_x.denom }, amount },
                None,
                slippage_tolerance,
                None,
            )?;
            messages.push(swap_msg);
        }
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "optimal_swap"))
}

fn calculate_optimal_swap(
    mut amount_x: Uint128,
    mut amount_y: Uint128,
    mut pool_x: Uint128,
    mut pool_y: Uint128,
) -> (Uint128, bool) {

    let area_x = Uint256::from(amount_x) * Uint256::from(pool_y);
    let area_y = Uint256::from(amount_y) * Uint256::from(pool_x);

    let mut invert = false;
    if area_y > area_x {
        (amount_x, amount_y) = (amount_y, amount_x);
        (pool_x, pool_y) = (pool_y, pool_x);
        invert = true;
    }

    let match_x = amount_y.multiply_ratio(pool_x, pool_y);
    let swap_x = (amount_x - match_x).multiply_ratio(10000u128, 19985u128);

    (swap_x, invert)
}

/// ## Description
/// Provides liquidity on the pair contract to get LP token.
fn provide_liquidity(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    _info: MessageInfo,
    market_maker: MarketMaker,
    [prev_x, prev_y]: [Coin; 2],
    slippage_tolerance: Option<Decimal>,
) -> Result<Response, ContractError> {
    let amount_x = deps.querier.query_balance(&env.contract.address, &prev_x.denom)?.amount - prev_x.amount;
    let amount_y = deps.querier.query_balance(&env.contract.address, &prev_y.denom)?.amount - prev_y.amount;

    let provide_liquidity_msg = market_maker.deposit(
        vec![
            Coin { denom: prev_x.denom, amount: amount_x },
            Coin { denom: prev_y.denom, amount: amount_y },
        ],
        slippage_tolerance,
        None,
    )?;

    Ok(Response::new()
        .add_message(provide_liquidity_msg)
        .add_attribute("action", "provide_liquidity"))
}

/// ## Description
/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::CompoundSimulation { market_maker, rewards } =>
            to_binary(&query_compound_simulation(deps, market_maker, rewards)?),
    }
}

fn query_compound_simulation(
    deps: Deps<KujiraQuery>,
    market_maker: String,
    rewards: Vec<Coin>,
) -> StdResult<CompoundSimulationResponse> {
    let market_maker = MarketMaker(deps.api.addr_validate(&market_maker)?);
    let config = CONFIG.load(deps.storage)?;
    let mm_config = market_maker.query_config(&deps.querier)?;
    let [token_x, token_y] = mm_config.denoms;

    // Swap reward to asset in the pair
    let mut provide_x = Uint128::zero();
    let mut provide_y = Uint128::zero();
    for fund in rewards.iter() {
        if fund.denom == token_x.to_string() {
            provide_x += fund.amount;
        } else if fund.denom == token_y.to_string() {
            provide_y += fund.amount;
        } else if let Ok(amount_x) = config.router.try_swap_simulation(&deps.querier, fund.denom.clone(), token_x.clone(), fund.amount) {
            provide_x += amount_x;
        } else if let Ok(amount_y) = config.router.try_swap_simulation(&deps.querier, fund.denom.clone(), token_y.clone(), fund.amount) {
            provide_y += amount_y;
        } else {
            return Err(StdError::generic_err("Invalid asset"))
        }
    }

    let pool = market_maker.query_pool(&deps.querier)?;
    let [pool_x, pool_y] = pool.balances;
    let (swap_amount, invert) = calculate_optimal_swap(
        provide_x,
        provide_y,
        pool_x,
        pool_y,
    );
    let mut return_amount = Uint128::zero();
    if !swap_amount.is_zero() {
        if invert {
            return_amount = Pair(mm_config.fin_contract)
                .simulate(
                    &deps.querier,
                    &Asset {
                        info: AssetInfo::NativeToken { denom: token_y },
                        amount: swap_amount,
                    })?.return_amount.try_into()?;
            provide_x += return_amount;
            provide_y -= swap_amount;
        } else {
            return_amount = Pair(mm_config.fin_contract)
                .simulate(
                    &deps.querier,
                    &Asset {
                        info: AssetInfo::NativeToken { denom: token_x },
                        amount: swap_amount,
                    })?.return_amount.try_into()?;
            provide_y += return_amount;
            provide_x -= swap_amount;
        }
    }

    let supply = market_maker.query_lp_supply(&deps.querier)?;
    let lp_amount = min(
        provide_x.multiply_ratio(supply.amount.amount, pool_x),
        provide_y.multiply_ratio(supply.amount.amount, pool_y),
    );

    Ok(CompoundSimulationResponse {
        lp_amount,
        swap_asset_a_amount: if invert { Uint128::zero() } else { swap_amount },
        swap_asset_b_amount: if invert { swap_amount } else { Uint128::zero() },
        return_a_amount: if invert { return_amount } else { Uint128::zero() },
        return_b_amount: if invert { Uint128::zero() } else { return_amount },
    })
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}
