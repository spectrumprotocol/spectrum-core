use cosmwasm_std::{Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};
use astroport::asset::{addr_validate_to_lower, AssetInfo};
use crate::error::ContractError;
use crate::model::{Config, PoolConfig};
use crate::state::{CONFIG, POOL_CONFIG};

pub fn validate_percentage(value: Decimal, field: &str) -> StdResult<()> {
    if value > Decimal::one() {
        Err(StdError::generic_err(field.to_string() + " cannot greater than 1"))
    } else {
        Ok(())
    }
}
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    controller: Option<String>,
    fee_rate: Option<Decimal>,
) -> Result<Response, ContractError> {

    // only owner can update
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(controller) = controller {
        config.controller = addr_validate_to_lower(deps.api, &controller)?;
    }

    if let Some(fee_rate) = fee_rate {
        validate_percentage(fee_rate, "fee_rate")?;
        config.fee_rate = fee_rate;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

pub fn execute_update_parameters(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    max_quota: Option<Uint128>,
    staker_rate: Option<Decimal>,
) -> Result<Response, ContractError> {

    // only controller can update
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.controller {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(max_quota) = max_quota {
        config.max_quota = max_quota;
    }

    if let Some(staker_rate) = staker_rate {
        validate_percentage(staker_rate, "staker_rate")?;
        config.staker_rate = staker_rate;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

pub fn execute_update_pool_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    lp_token: String,
    asset_rewards: Option<Vec<AssetInfo>>,
) -> Result<Response, ContractError> {

    // only controller can update
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.controller {
        return Err(ContractError::Unauthorized {});
    }

    // load data
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let mut pool_config = POOL_CONFIG.may_load(deps.storage, &lp_token)?
        .unwrap_or_default();

    if let Some(asset_rewards) = asset_rewards {
        pool_config.asset_rewards = asset_rewards;
    }

    POOL_CONFIG.save(deps.storage, &lp_token, &pool_config)?;

    Ok(Response::default())
}

pub fn query_config(
    deps: Deps,
    _env: Env,
) -> StdResult<Config> {
    CONFIG.load(deps.storage)
}

pub fn query_pool_config(
    deps: Deps,
    _env: Env,
    lp_token: String,
) -> StdResult<PoolConfig> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    POOL_CONFIG.load(deps.storage, &lp_token)
}
