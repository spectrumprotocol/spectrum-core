use cosmwasm_std::{attr, entry_point, to_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Empty, Order};
use cw_storage_plus::Bound;
use kujira::msg::{KujiraMsg};
use kujira::query::KujiraQuery;
use spectrum::adapters::kujira::staking::Staking;

use crate::{
    bond::{bond, bond_assets, bond_to},
    compound::{compound, stake},
    error::ContractError,
    state::{Config, CONFIG},
};

use crate::bond::{query_reward_info, unbond};
use spectrum::compound_farm::{
    CallbackMsg, ExecuteMsg, InstantiateMsg, QueryMsg,
};
use spectrum::compound_proxy::Compounder;
use spectrum::ownership::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use spectrum::router::Router;
use crate::state::{POOL, PoolInfo};

/// ## Description
/// Validates that decimal value is in the range 0 to 1
fn validate_percentage(value: Decimal, field: &str) -> StdResult<()> {
    if value > Decimal::one() {
        Err(StdError::generic_err(field.to_string() + " must be 0 to 1"))
    } else {
        Ok(())
    }
}

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    validate_percentage(msg.fee, "fee")?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            staking: Staking(deps.api.addr_validate(&msg.staking)?),
            compound_proxy: Compounder(deps.api.addr_validate(&msg.compound_proxy)?),
            controller: deps.api.addr_validate(&msg.controller)?,
            fee: msg.fee,
            fee_collector: deps.api.addr_validate(&msg.fee_collector)?,
            router: Router(deps.api.addr_validate(&msg.router)?),
        },
    )?;

    Ok(Response::default())
}

/// ## Description
/// Exposes execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig {
            compound_proxy,
            controller,
            fee,
            fee_collector,
        } => update_config(deps, info, compound_proxy, controller, fee, fee_collector),
        ExecuteMsg::Unbond { } => unbond(deps, env, info),
        ExecuteMsg::Bond { staker_addr } => bond(deps, env, info, staker_addr),
        ExecuteMsg::BondAssets {
            market_maker,
            minimum_receive,
            no_swap,
            slippage_tolerance,
        } => bond_assets(
            deps,
            env,
            info,
            market_maker,
            minimum_receive,
            no_swap,
            slippage_tolerance,
        ),
        ExecuteMsg::Compound {
            market_maker,
            minimum_receive,
            slippage_tolerance,
        } => compound(deps, env, info, market_maker, minimum_receive, slippage_tolerance),
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

/// ## Description
/// Updates contract config. Returns a [`ContractError`] on failure or the [`CONFIG`] data will be updated.
#[allow(clippy::too_many_arguments)]
fn update_config(
    deps: DepsMut<KujiraQuery>,
    info: MessageInfo,
    compound_proxy: Option<String>,
    controller: Option<String>,
    fee: Option<Decimal>,
    fee_collector: Option<String>,
) -> Result<Response<KujiraMsg>, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(compound_proxy) = compound_proxy {
        config.compound_proxy = Compounder(deps.api.addr_validate(&compound_proxy)?);
    }

    if let Some(controller) = controller {
        config.controller = deps.api.addr_validate(&controller)?;
    }

    if let Some(fee) = fee {
        validate_percentage(fee, "fee")?;
        config.fee = fee;
    }

    if let Some(fee_collector) = fee_collector {
        config.fee_collector = deps.api.addr_validate(&fee_collector)?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

/// # Description
/// Handle the callbacks describes in the [`CallbackMsg`]. Returns an [`ContractError`] on failure, otherwise returns the [`Response`]
pub fn handle_callback(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    // Callback functions can only be called by this contract itself
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    match msg {
        CallbackMsg::Stake {
            prev_balance,
            minimum_receive,
        } => stake(deps, env, info, prev_balance, minimum_receive),
        CallbackMsg::BondTo {
            to,
            prev_balance,
            minimum_receive,
        } => bond_to(deps, env, info, to, prev_balance, minimum_receive),
    }
}

/// ## Description
/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::RewardInfo { staker_addr, limit, start_after } =>
            to_binary(&query_reward_info(deps, env, staker_addr, limit, start_after)?),
        QueryMsg::Pools { limit, start_after } =>
            to_binary(&query_pools(deps, limit, start_after)?),
    }
}

const DEFAULT_LIMIT: u8 = 50;
const MAX_LIMIT: u8 = 50;
fn query_pools(deps: Deps<KujiraQuery>, limit: Option<u8>, start_after: Option<String>) -> StdResult<Vec<PoolInfo>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(|s| Bound::ExclusiveRaw(s.into()));

    let pools = POOL
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|it| {
            let (_, pool) = it?;
            Ok(pool)
        })
        .collect::<StdResult<_>>()?;
    Ok(pools)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> StdResult<Response> {
    Ok(Response::default())
}
