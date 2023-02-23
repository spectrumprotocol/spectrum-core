use std::cmp::Ordering;
use crate::error::ContractError;
use crate::state::{Config, CONFIG, ROUTES};

use cosmwasm_std::{entry_point, to_binary, Addr, Binary, Deps, DepsMut, Env, Fraction, MessageInfo, Response, StdResult, Uint128, Empty, Coin, CosmosMsg, BankMsg, Decimal256, Uint256, StdError, Api, QuerierWrapper, Order};
use cw_storage_plus::Bound;
use spectrum::router::{ExecuteMsg, InstantiateMsg, QueryMsg, MAX_ASSETS, CallbackMsg, SwapOperation, SwapOperationRequest, Route};

use kujira::asset::{Asset, AssetInfo};
use kujira::denom::Denom;
use kujira::fin::SimulationResponse;
use spectrum::ownership::{claim_ownership, drop_ownership_proposal, propose_new_owner};

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

/// ## Description
/// Exposes execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpsertRoute { operations } => upsert_route(deps, env, info, operations),
        ExecuteMsg::RemoveRoute { denoms } => remove_route(deps, env, info, denoms),
        ExecuteMsg::Swap {
            belief_price,
            max_spread,
            to,
            ask,
        } => swap(
                deps,
                env,
                info,
                belief_price,
                max_spread,
                to,
                ask,
            ),
        ExecuteMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            max_spread,
            to,
        } => execute_swap_operations(deps, env, info, operations, minimum_receive, max_spread, to),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config = CONFIG.load(deps.storage)?;

            Ok(propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
            )?)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config = CONFIG.load(deps.storage)?;

            Ok(drop_ownership_proposal(deps, info, config.owner)?)
        }
        ExecuteMsg::ClaimOwnership {} => {
            let sender = info.sender.clone();
            let res = claim_ownership(deps.storage, info, env)?;

            let mut config = CONFIG.load(deps.storage)?;
            config.owner = sender;
            CONFIG.save(deps.storage, &config)?;
            Ok(res)
        }
        ExecuteMsg::Callback(msg) => handle_callback(deps, env, info, msg),
    }
}

/// # Description
/// Handle the callbacks describes in the [`CallbackMsg`]. Returns an [`ContractError`] on failure, otherwise returns the [`Response`]
pub fn handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Callback functions can only be called by this contract itself
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    match msg {
        CallbackMsg::Swap {
            previous_balance,
            operations,
            to,
            minimum_receive,
            max_spread,
        } => callback_swap(deps, env, info, previous_balance, operations, to, minimum_receive, max_spread),
    }
}

fn validate_pair(
    querier: &QuerierWrapper,
    op: &SwapOperation,
) -> Result<i8, ContractError> {
    let config = op.pair.query_config(querier)?;
    let (compat, invert) = op.get_key_compat(&config.denoms[0], &config.denoms[1]);
    if !compat {
        return Err(ContractError::InvalidPair {})
    }
    if invert {
        Ok(-config.decimal_delta)
    } else {
        Ok(config.decimal_delta)
    }
}

fn validate_route(
    querier: &QuerierWrapper,
    api: &dyn Api,
    operations: Vec<SwapOperationRequest>,
) -> Result<(Vec<SwapOperation>, i8), ContractError> {

    // Validate swap assets
    match operations.split_first() {
        None => Err(ContractError::MustProvideNAssets {}),
        Some((head, tails)) => {
            if tails.len() + 1 > MAX_ASSETS {
                return Err(ContractError::SwapLimitExceeded {});
            }

            let validated_head = head.validate(api)?;
            let mut decimal_delta = validate_pair(querier, &validated_head)?;
            let mut validated_ops = vec![
                validated_head,
            ];
            let mut last_ask = &head.ask;
            for op in tails {
                if !last_ask.eq(&op.offer) {
                    return Err(ContractError::InvalidOperations {});
                }
                let validated_op = op.validate(api)?;
                decimal_delta += validate_pair(querier, &validated_op)?;
                validated_ops.push(validated_op);
                last_ask = &op.ask;
            }
            Ok((validated_ops, decimal_delta))
        }
    }
}

fn get_key(
    offer: &Denom,
    ask: &Denom,
) -> (String, bool) {
    let offer = offer.to_string();
    let ask = ask.to_string();
    if offer > ask {
        (format!("{ask}{offer}"), true)
    } else {
        (format!("{offer}{ask}"), false)
    }
}

fn invert_operations(
    operations: &[SwapOperation],
) -> Vec<SwapOperation> {
    operations.iter()
        .map(|it| it.rev())
        .rev()
        .collect()
}

fn upsert_route(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    operations: Vec<SwapOperationRequest>,
) -> Result<Response, ContractError> {

    let config = CONFIG.load(deps.storage)?;
    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let (mut operations, mut decimal_delta) = validate_route(&deps.querier, deps.api, operations)?;
    let (key, invert) = get_key(&operations[0].offer, &operations[operations.len() - 1].ask);
    if invert {
        operations = invert_operations(&operations);
        decimal_delta = -decimal_delta;
    }

    ROUTES.save(deps.storage, key.clone(), &Route {
        key,
        operations,
        decimal_delta,
    })?;

    Ok(Response::default())
}

fn remove_route(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    denoms: [Denom; 2],
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let (key, _) = get_key(&denoms[0], &denoms[1]);
    ROUTES.remove(deps.storage, key);
    Ok(Response::default())
}

/// ## Description
/// Performs an swap operation with the specified parameters. CONTRACT - a user must do token approval.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
#[allow(clippy::too_many_arguments)]
fn swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    belief_price: Option<Decimal256>,
    max_spread: Option<Decimal256>,
    to: Option<String>,
    ask: Denom,
) -> Result<Response, ContractError> {
    let fund = match &info.funds[..] {
        [fund] => fund,
        _ => return Err(ContractError::InvalidFunds {}),
    };

    let (key, _) = get_key(&Denom::from(&fund.denom), &ask);
    let route = ROUTES.load(deps.storage, key)?;

    let (operations, decimal_delta) =
        if fund.denom.eq(&route.operations[0].offer.to_string()) {
            (route.operations, route.decimal_delta)
        } else {
            (invert_operations(&route.operations), -route.decimal_delta)
        };

    let to = match to {
        None => info.sender,
        Some(to) => deps.api.addr_validate(&to)?,
    };
    let swap_msg = if operations.len() == 1 {
        operations[0].pair.swap_msg(
            fund.clone(),
            belief_price,
            max_spread,
            Some(to),
        )?
    } else {
        let minimum_receive = match (belief_price, max_spread) {
            (Some(belief_price), Some(max_spread)) => {
                let minimum_receive = compute_minimum_receive(
                    fund.amount,
                    belief_price,
                    max_spread,
                    decimal_delta,
                )?;
                Some(minimum_receive)
            }
            (_, _) => None,
        };

        let previous_balance = deps.querier.query_balance(
            &env.contract.address,
            operations[0].offer.to_string(),
        )?;
        CallbackMsg::Swap {
            previous_balance,
            operations,
            to,
            minimum_receive,
            max_spread,
        }.to_cosmos_msg(&env.contract.address)?
    };

    Ok(Response::new()
        .add_message(swap_msg))
}

/// Computes minimum return amount from belief price and max spread
fn compute_minimum_receive(
    offer_amount: Uint128,
    belief_price: Decimal256,
    max_spread: Decimal256,
    decimal_delta: i8,
) -> StdResult<Uint128> {
    let dec_adj = match decimal_delta.cmp(&0) {
        Ordering::Less => Decimal256::from_ratio(1u128, 10u128.pow(-decimal_delta as u32)),
        Ordering::Equal => Decimal256::one(),
        Ordering::Greater => Decimal256::from_ratio(10u128.pow(decimal_delta as u32), 1u128),
    };
    let micro_price = Decimal256::from_ratio(dec_adj.numerator(), belief_price.numerator());
    let result = Uint256::from(offer_amount) * micro_price * (Decimal256::one() - max_spread);
    result.try_into().map_err(StdError::from)
}

fn execute_swap_operations(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operations: Vec<SwapOperationRequest>,
    minimum_receive: Option<Uint128>,
    max_spread: Option<Decimal256>,
    to: Option<String>,
) -> Result<Response, ContractError> {
    let fund = match &info.funds[..] {
        [fund] => fund,
        _ => return Err(ContractError::InvalidFunds {}),
    };
    let (operations, _) = validate_route(&deps.querier, deps.api, operations)?;
    if operations[0].offer.ne(&Denom::from(&fund.denom)) {
        return Err(ContractError::InvalidAsset {});
    }

    let to = match to {
        None => info.sender,
        Some(to) => deps.api.addr_validate(&to)?,
    };
    let previous_balance = deps.querier.query_balance(
        &env.contract.address,
        operations[0].offer.to_string(),
    )?;
    let swap_msg = CallbackMsg::Swap {
        previous_balance,
        operations,
        to,
        minimum_receive,
        max_spread,
    }.to_cosmos_msg(&env.contract.address)?;

    Ok(Response::new()
        .add_message(swap_msg))
}

#[allow(clippy::too_many_arguments)]
fn callback_swap(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    previous_balance: Coin,
    operations: Vec<SwapOperation>,
    to: Addr,
    minimum_receive: Option<Uint128>,
    max_spread: Option<Decimal256>,
) -> Result<Response, ContractError> {

    let current = deps.querier.query_balance(
        &env.contract.address,
        &previous_balance.denom,
    )?;
    let amount = current.amount - previous_balance.amount;
    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let res = match operations.split_first() {
        None => {
            if let Some(minimum_receive) = minimum_receive {
                if amount < minimum_receive {
                    return Err(ContractError::AssertionMinimumReceive {
                        amount: minimum_receive,
                        receive: amount,
                    });
                }
            }
            let transfer_msg = CosmosMsg::Bank(BankMsg::Send {
                to_address: to.to_string(),
                amount: vec![ Coin { denom: previous_balance.denom, amount } ],
            });
            Response::new()
                .add_message(transfer_msg)
        },
        Some((head, tails)) => {
            let previous_balance = deps.querier.query_balance(
                &env.contract.address,
                head.ask.to_string(),
            )?;
            Response::new()
                .add_message(head.pair.swap_msg(
                    Coin { denom: previous_balance.denom.to_string(), amount },
                    None,
                    max_spread,
                    None,
                )?)
                .add_message(CallbackMsg::Swap {
                    previous_balance,
                    operations: tails.to_vec(),
                    to,
                    minimum_receive,
                    max_spread,
                }.to_cosmos_msg(&env.contract.address)?)
        },
    };

    Ok(res)
}

const DEFAULT_LIMIT: u8 = 50;
const MAX_LIMIT: u8 = 50;

/// ## Description
/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} =>
            to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Route { denoms } =>
            to_binary(&query_route(deps, denoms)?),
        QueryMsg::Routes { limit, start_after } =>
            to_binary(&query_routes(deps, limit, start_after)?),
        QueryMsg::Simulation { offer_asset, ask } =>
            to_binary(&query_simulation(deps, offer_asset, ask)?),
        QueryMsg::SimulateSwapOperations { offer_amount, operations } =>
            to_binary(&simulate_swap_operations(deps, offer_amount, operations)?),
    }
}

fn query_route(deps: Deps, denoms: [Denom; 2]) -> StdResult<Route> {
    let (key, _) = get_key(&denoms[0], &denoms[1]);
    ROUTES.load(deps.storage, key)
}

fn query_routes(deps: Deps, limit: Option<u8>, start_after: Option<String>) -> StdResult<Vec<Route>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into()));

    let routes = ROUTES
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|it| {
            let (_, route) = it?;
            Ok(route)
        })
        .collect::<StdResult<_>>()?;
    Ok(routes)
}

fn simulation_internal(
    querier: &QuerierWrapper,
    offer_amount: Uint128,
    operations: Vec<SwapOperation>,
) -> StdResult<SimulationResponse> {
    let mut offer_amount: Uint256 = offer_amount.into();
    let mut spread_amount = Uint256::zero();
    let mut commission_amount = Uint256::zero();
    for op in operations {
        let sim_result = op.pair.simulate(querier, &Asset {
            amount: offer_amount.try_into()?,
            info: AssetInfo::NativeToken { denom: op.offer },
        })?;
        let total_amount = sim_result.return_amount + sim_result.commission_amount + sim_result.spread_amount;
        commission_amount = sim_result.commission_amount + commission_amount.multiply_ratio(total_amount, offer_amount);
        spread_amount = sim_result.spread_amount + spread_amount.multiply_ratio(total_amount, offer_amount);
        offer_amount = sim_result.return_amount;
    }

    Ok(SimulationResponse {
        return_amount: offer_amount,
        spread_amount,
        commission_amount,
    })
}

/// ## Description
/// Returns information about a swap simulation in a [`SimulationResponse`] object.
fn query_simulation(deps: Deps, offer_asset: Asset, ask: Denom) -> StdResult<SimulationResponse> {
    let fund = match &offer_asset.info {
        AssetInfo::NativeToken { denom } => Coin { denom: denom.to_string(), amount: offer_asset.amount },
    };
    let (key, _) = get_key(&Denom::from(&fund.denom), &ask);
    let route = ROUTES.load(deps.storage, key)?;

    let operations =
        if fund.denom.eq(&route.operations[0].offer.to_string()) {
            route.operations
        } else {
            invert_operations(&route.operations)
        };

    simulation_internal(&deps.querier, fund.amount, operations)
}

fn simulate_swap_operations(
    deps: Deps,
    offer_amount: Uint128,
    operations: Vec<SwapOperationRequest>,
) -> StdResult<SimulationResponse> {
    let (operations, _) = validate_route(&deps.querier, deps.api, operations)
        .map_err(|err| StdError::generic_err(format!("{err}")))?;

    simulation_internal(&deps.querier, offer_amount, operations)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimum_receive() {
        let min_receive = compute_minimum_receive(
            Uint128::from(1234_567u128),
            Decimal256::permille(10000_000),
            Decimal256::zero(),
            -2,
        );
        assert_eq!(min_receive, Uint128::from(0_12345u128));

        let min_receive = compute_minimum_receive(
            Uint128::from(12_34567u128),
            Decimal256::permille(0_001),
            Decimal256::zero(),
            5,
        );
        assert_eq!(min_receive, Uint128::from(12345u128));
    }
}
