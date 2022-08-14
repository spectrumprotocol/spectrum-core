use astroport::asset::addr_validate_to_lower;

use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128,
};

use crate::{
    bond::{bond, bond_assets, bond_to},
    compound::{compound, stake},
    error::ContractError,
    ownership::{claim_ownership, drop_ownership_proposal, propose_new_owner},
    state::{Config, State, CONFIG, OWNERSHIP_PROPOSAL},
};

use cw20::Cw20ReceiveMsg;
use spectrum::adapters::generator::Generator;

use crate::bond::{query_reward_info, unbond};
use crate::state::STATE;
use spectrum::astroport_farm::{
    CallbackMsg, InstantiateMsg, Cw20HookMsg, ExecuteMsg, MigrateMsg, QueryMsg,
    StateInfo,
};
use spectrum::compound_proxy::Compounder;

/// (we require 0-1)
fn validate_percentage(value: Decimal, field: &str) -> StdResult<()> {
    if value > Decimal::one() {
        Err(StdError::generic_err(field.to_string() + " must be 0 to 1"))
    } else {
        Ok(())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    validate_percentage(msg.fee, "community_fee")?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: addr_validate_to_lower(deps.api, &msg.owner)?,
            staking_contract: Generator(addr_validate_to_lower(deps.api, &msg.staking_contract)?),
            compound_proxy: Compounder(addr_validate_to_lower(deps.api, &msg.compound_proxy)?),
            controller: addr_validate_to_lower(deps.api, &msg.controller)?,
            fee: msg.fee,
            fee_collector: addr_validate_to_lower(deps.api, &msg.fee_collector)?,
            liquidity_token: addr_validate_to_lower(deps.api, &msg.liquidity_token)?,
            base_reward_token: addr_validate_to_lower(deps.api, &msg.base_reward_token)?,
        },
    )?;

    STATE.save(
        deps.storage,
        &State {
            earning: Uint128::zero(),
            total_bond_share: Uint128::zero(),
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::UpdateConfig {
            compound_proxy,
            controller,
            fee,
            fee_collector,
        } => update_config(
            deps,
            info,
            compound_proxy,
            controller,
            fee,
            fee_collector,
        ),
        ExecuteMsg::Unbond { amount } => unbond(deps, env, info, amount),
        ExecuteMsg::BondAssets { assets, minimum_receive } => bond_assets(deps, env, info, assets, minimum_receive),
        ExecuteMsg::Compound { minimum_receive } => compound(deps, env, info, minimum_receive),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config: Config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(|e| e.into())
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(|e| e.into())
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(|e| e.into())
        }
        ExecuteMsg::Callback(msg) => handle_callback(deps, env, info, msg),
    }
}

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Bond { staker_addr }) => bond(
            deps,
            env,
            info,
            staker_addr.unwrap_or(cw20_msg.sender),
            cw20_msg.amount,
        ),
        Err(_) => Err(ContractError::InvalidMessage {}),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    compound_proxy: Option<String>,
    controller: Option<String>,
    fee: Option<Decimal>,
    fee_collector: Option<String>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(compound_proxy) = compound_proxy {
        config.compound_proxy = Compounder(addr_validate_to_lower(deps.api, &compound_proxy)?);
    }

    if let Some(controller) = controller {
        config.controller = addr_validate_to_lower(deps.api, &controller)?;
    }

    if let Some(fee) = fee {
        validate_percentage(fee, "fee")?;
        config.fee = fee;
    }

    if let Some(fee_collector) = fee_collector {
        config.fee_collector = addr_validate_to_lower(deps.api, &fee_collector)?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

/// # Description
/// Handle the callbacks describes in the [`CallbackMsg`]. Returns an [`ContractError`] on failure, otherwise returns the [`Response`]
/// object with the specified submessages if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`CallbackMsg`]. Sets the callback action.
///
/// ## Executor
/// Callback functions can only be called this contract itself
pub fn handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Callback functions can only be called this contract itself
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    match msg {
        CallbackMsg::Stake {
            prev_balance,
            minimum_receive,
        } => stake(deps, env, info, prev_balance, minimum_receive),
        CallbackMsg::BondTo { to, prev_balance, minimum_receive } => bond_to(deps, env, info, to, prev_balance, minimum_receive),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::RewardInfo { staker_addr } => {
            to_binary(&query_reward_info(deps, env, staker_addr)?)
        }
        QueryMsg::State {} => to_binary(&query_state(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

fn query_state(deps: Deps) -> StdResult<StateInfo> {
    let state = STATE.load(deps.storage)?;
    Ok(StateInfo {
        total_bond_share: state.total_bond_share,
        earning: state.earning,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
