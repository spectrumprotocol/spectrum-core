use crate::error::ContractError;
use crate::state::{BRIDGES};
use cosmwasm_std::{to_binary, StdResult, WasmMsg, CosmosMsg, Addr, Deps};
use kujira::denom::Denom;
use spectrum::fees_collector::ExecuteMsg;
use spectrum::router::Router;

/// The default bridge depth for a fee token
pub const BRIDGES_INITIAL_DEPTH: u64 = 0;
/// Maximum amount of bridges to use in a multi-hop swap
pub const BRIDGES_MAX_DEPTH: u64 = 2;
/// Swap execution depth limit
pub const BRIDGES_EXECUTION_MAX_DEPTH: u64 = 3;

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
