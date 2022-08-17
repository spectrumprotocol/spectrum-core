use cosmwasm_std::{Deps, Env, StdResult};
use astroport::asset::addr_validate_to_lower;
use crate::model::{PoolInfo, RewardInfo, State, UserInfo};
use crate::state::{POOL_INFO, REWARD_INFO, STATE, USER_INFO};

pub fn query_pool_info(
    deps: Deps,
    _env: Env,
    lp_token: String,
) -> StdResult<PoolInfo> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    POOL_INFO.load(deps.storage, &lp_token)
}

pub fn query_user_info(
    deps: Deps,
    _env: Env,
    lp_token: String,
    user: String,
) -> StdResult<UserInfo> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let user = addr_validate_to_lower(deps.api, &user)?;
    USER_INFO.load(deps.storage, (&lp_token, &user))
}

pub fn query_reward_info(
    deps: Deps,
    _env: Env,
    token: String,
) -> StdResult<RewardInfo> {
    let token = addr_validate_to_lower(deps.api, &token)?;
    REWARD_INFO.load(deps.storage, &token)
}

pub fn query_state(
    deps: Deps,
    _env: Env,
) -> StdResult<State> {
    STATE.load(deps.storage)
}
