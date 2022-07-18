use astroport::asset::addr_validate_to_lower;

use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128,
};

use crate::{
    bond::bond,
    compound::{compound, stake},
    error::ContractError,
    state::{Config, PoolInfo, State, CONFIG},
};

use cw20::Cw20ReceiveMsg;

use crate::bond::{query_reward_info, unbond};
use crate::state::{POOL_INFO, STATE};
use spectrum::astroport_farm::{
    CallbackMsg, ConfigInfo, Cw20HookMsg, ExecuteMsg, MigrateMsg, PoolItem, PoolResponse, QueryMsg,
    StateInfo,
};

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
    msg: ConfigInfo,
) -> Result<Response, ContractError> {
    validate_percentage(msg.community_fee, "community_fee")?;
    validate_percentage(msg.platform_fee, "platform_fee")?;
    validate_percentage(msg.controller_fee, "controller_fee")?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: addr_validate_to_lower(deps.api, &msg.owner)?,
            spectrum_gov: addr_validate_to_lower(deps.api, &msg.spectrum_gov)?,
            astroport_generator: addr_validate_to_lower(deps.api, &msg.astroport_generator)?,
            astro_token: addr_validate_to_lower(deps.api, &msg.astro_token)?,
            compound_proxy: addr_validate_to_lower(deps.api, &msg.compound_proxy)?,
            platform: addr_validate_to_lower(deps.api, &msg.platform)?,
            controller: addr_validate_to_lower(deps.api, &msg.controller)?,
            base_denom: msg.base_denom,
            community_fee: msg.community_fee,
            platform_fee: msg.platform_fee,
            controller_fee: msg.controller_fee,
            community_fee_collector: addr_validate_to_lower(
                deps.api,
                &msg.community_fee_collector,
            )?,
            platform_fee_collector: addr_validate_to_lower(deps.api, &msg.platform_fee_collector)?,
            controller_fee_collector: addr_validate_to_lower(
                deps.api,
                &msg.controller_fee_collector,
            )?,
            pair_contract: addr_validate_to_lower(deps.api, &msg.pair_contract)?,
        },
    )?;

    STATE.save(
        deps.storage,
        &State {
            earning: Uint128::zero(),
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
            owner,
            controller,
            community_fee,
            platform_fee,
            controller_fee,
        } => update_config(
            deps,
            info,
            owner,
            controller,
            community_fee,
            platform_fee,
            controller_fee,
        ),
        ExecuteMsg::RegisterAsset {
            asset_token,
            staking_token,
        } => register_asset(deps, env, info, asset_token, staking_token),
        ExecuteMsg::Unbond { amount } => unbond(deps, env, info, amount),
        ExecuteMsg::Compound { minimum_receive } => compound(deps, env, info, minimum_receive),
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
    owner: Option<String>,
    controller: Option<String>,
    community_fee: Option<Decimal>,
    platform_fee: Option<Decimal>,
    controller_fee: Option<Decimal>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if addr_validate_to_lower(deps.api, info.sender.as_str())? != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(owner) = owner {
        if config.owner == config.spectrum_gov {
            return Err(ContractError::CannotUpdateOwner {});
        }
        config.owner = addr_validate_to_lower(deps.api, &owner)?;
    }

    if let Some(controller) = controller {
        config.controller = addr_validate_to_lower(deps.api, &controller)?;
    }

    if let Some(community_fee) = community_fee {
        validate_percentage(community_fee, "community_fee")?;
        config.community_fee = community_fee;
    }

    if let Some(platform_fee) = platform_fee {
        validate_percentage(platform_fee, "platform_fee")?;
        config.platform_fee = platform_fee;
    }

    if let Some(controller_fee) = controller_fee {
        validate_percentage(controller_fee, "controller_fee")?;
        config.controller_fee = controller_fee;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

fn register_asset(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    asset_token: String,
    staking_token: String,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let pool = POOL_INFO.may_load(deps.storage)?;

    if pool.is_some() {
        return Err(ContractError::AssetRegistered {});
    } else {
        let pool_info = POOL_INFO
            .may_load(deps.storage)?
            .unwrap_or_else(|| PoolInfo {
                asset_token: addr_validate_to_lower(deps.api, &asset_token).unwrap(),
                staking_token: addr_validate_to_lower(deps.api, &staking_token).unwrap(),
                total_bond_share: Uint128::zero(),
            });

        POOL_INFO.save(deps.storage, &pool_info)?;
        Ok(Response::new().add_attributes(vec![
            attr("action", "register_asset"),
            attr("asset_token", asset_token),
        ]))
    }
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
        CallbackMsg::Stake {} => stake(deps, env, info),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Pool {} => to_binary(&query_pools(deps)?),
        QueryMsg::RewardInfo { staker_addr } => {
            to_binary(&query_reward_info(deps, env, staker_addr)?)
        }
        QueryMsg::State {} => to_binary(&query_state(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigInfo> {
    let config = CONFIG.load(deps.storage)?;
    let resp = ConfigInfo {
        owner: config.owner.to_string(),
        astroport_generator: config.astroport_generator.to_string(),
        astro_token: config.astro_token.to_string(),
        spectrum_gov: config.spectrum_gov.to_string(),
        compound_proxy: config.compound_proxy.to_string(),
        platform: config.platform.to_string(),
        controller: config.controller.to_string(),
        base_denom: config.base_denom,
        community_fee: config.community_fee,
        platform_fee: config.platform_fee,
        controller_fee: config.controller_fee,
        community_fee_collector: config.community_fee_collector.to_string(),
        platform_fee_collector: config.platform_fee_collector.to_string(),
        controller_fee_collector: config.controller_fee_collector.to_string(),
        pair_contract: config.pair_contract.to_string(),
    };

    Ok(resp)
}

fn query_pools(deps: Deps) -> StdResult<PoolResponse> {
    let pool = POOL_INFO.load(deps.storage)?;
    Ok(PoolResponse {
        pool: PoolItem {
            asset_token: pool.asset_token.to_string(),
            staking_token: pool.staking_token.to_string(),
            total_bond_share: pool.total_bond_share,
        },
    })
}

fn query_state(deps: Deps) -> StdResult<StateInfo> {
    let state = STATE.load(deps.storage)?;
    Ok(StateInfo {
        earning: state.earning,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
