use astroport::{generator::{PendingTokenResponse, QueryMsg as AstroportQueryMsg, RewardInfoResponse}, asset::PairInfo, pair::{QueryMsg as PairQueryMsg}};
use cosmwasm_std::{to_binary, Addr, Deps, QueryRequest, StdResult, Uint128, WasmQuery};

pub fn query_astroport_pending_token(
    deps: Deps,
    lp_token: &Addr,
    staker: &Addr,
    astroport_generator: &Addr,
) -> StdResult<PendingTokenResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: astroport_generator.to_string(),
        msg: to_binary(&AstroportQueryMsg::PendingToken {
            lp_token: lp_token.to_string(),
            user: staker.to_string(),
        })?,
    }))
}

pub fn query_astroport_pool_balance(
    deps: Deps,
    lp_token: &Addr,
    staker: &Addr,
    astroport_generator: &Addr,
) -> StdResult<Uint128> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: astroport_generator.to_string(),
        msg: to_binary(&AstroportQueryMsg::Deposit {
            lp_token: lp_token.to_string(),
            user: staker.to_string(),
        })?,
    }))
}

pub fn query_astroport_reward_info(
    deps: Deps,
    lp_token: &Addr,
    astroport_generator: &Addr,
) -> StdResult<RewardInfoResponse> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: astroport_generator.to_string(),
        msg: to_binary(&AstroportQueryMsg::RewardInfo {
            lp_token: lp_token.to_string(),
        })?,
    }))
}

/// ## Description
/// Returns information about the pair described in the structure [`PairInfo`] according to the specified parameters in the `pair_contract` variable.
/// ## Params
/// `pair_contract` it is the type of [`Addr`].
pub fn query_pair_info(deps: Deps, pair_contract: &Addr) -> StdResult<PairInfo> {
    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&PairQueryMsg::Pair {})?,
    }))
}