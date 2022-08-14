use cosmwasm_std::{Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Uint128};
use astroport::asset::{addr_validate_to_lower};
use crate::error::ContractError;
use crate::model::{Config};
use crate::state::{CONFIG};

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
    boost_fee: Option<Decimal>,
) -> Result<Response, ContractError> {

    // only owner can update
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(controller) = controller {
        config.controller = addr_validate_to_lower(deps.api, &controller)?;
    }

    if let Some(boost_fee) = boost_fee {
        validate_percentage(boost_fee, "boost_fee")?;
        config.boost_fee = boost_fee;
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

pub fn query_config(
    deps: Deps,
    _env: Env,
) -> StdResult<Config> {
    CONFIG.load(deps.storage)
}
