use crate::error::ContractError;
use crate::state::{BRIDGES};
use cosmwasm_std::{to_binary, StdResult, Uint128, WasmMsg, CosmosMsg, Addr, QuerierWrapper, Coin, Deps};
use kujira::asset::{Asset, AssetInfo};
use kujira::denom::Denom;
use spectrum::fees_collector::ExecuteMsg;
use spectrum::router::Router;

/// The default bridge depth for a fee token
pub const BRIDGES_INITIAL_DEPTH: u64 = 0;
/// Maximum amount of bridges to use in a multi-hop swap
pub const BRIDGES_MAX_DEPTH: u64 = 2;
/// Swap execution depth limit
pub const BRIDGES_EXECUTION_MAX_DEPTH: u64 = 3;

/// Creates swap message
pub fn try_build_swap_msg(
    querier: &QuerierWrapper,
    router: &Router,
    from: Denom,
    to: Denom,
    amount: Uint128,
) -> Result<CosmosMsg, ContractError> {
    router.query_route(querier, [from.clone(), to.clone()])?;
    let msg = router.swap_msg(
        Coin { denom: from.to_string(), amount },
        to,
        None,
        None,
        None,
    )?;
    Ok(msg)
}

pub fn try_swap_simulation(
    querier: &QuerierWrapper,
    router: &Router,
    from: String,
    to: Denom,
    amount: Uint128,
) -> StdResult<Uint128> {
    let result = router.simulate(
        querier,
        Asset { info: AssetInfo::NativeToken { denom: from.into() }, amount },
        to,
    )?;
    Ok(result.return_amount.try_into()?)
}

/// Creates swap message via bridge token pair
pub fn build_swap_bridge_msg(
    contract_addr: &Addr,
    bridge_assets: Vec<Denom>,
    depth: u64,
) -> StdResult<CosmosMsg> {
    let msg: CosmosMsg =
        // Swap bridge assets
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(&ExecuteMsg::SwapBridgeAssets {
                assets: bridge_assets,
                depth,
            })?,
            funds: vec![],
        });

    Ok(msg)
}

/// Validates bridge token
pub fn validate_bridge(
    deps: Deps,
    router: &Router,
    from_token: &Denom,
    bridge_token: &Denom,
    stablecoin_token: &Denom,
    depth: u64,
) -> Result<(), ContractError> {
    // Check if the bridge pool exists
    router.query_route(&deps.querier, [from_token.clone(), bridge_token.clone()])?;

    // Check if the bridge token - stablecoin pool exists
    let stablecoin_pool = router.query_route(&deps.querier, [bridge_token.clone(), stablecoin_token.clone()]);
    if stablecoin_pool.is_err() {
        if depth >= BRIDGES_MAX_DEPTH {
            return Err(ContractError::MaxBridgeDepth(depth));
        }

        // Check if next level of bridge exists
        let next_bridge_token = BRIDGES
            .load(deps.storage, bridge_token.to_string())
            .map_err(|_| ContractError::InvalidBridgeDestination(from_token.clone()))?;

        validate_bridge(
            deps,
            router,
            bridge_token,
            &next_bridge_token,
            stablecoin_token,
            depth + 1,
        )?;
    }

    Ok(())
}
