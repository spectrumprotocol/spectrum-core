use crate::error::ContractError;
use crate::state::{Config, BRIDGES};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::Cw20HookMsg;
use astroport::querier::query_pair_info;
use cosmwasm_std::{to_binary, Coin, Deps, Env, StdResult, Uint128, WasmMsg, CosmosMsg};
use spectrum::fees_collector::ExecuteMsg;

/// The default bridge depth for a fee token
pub const BRIDGES_INITIAL_DEPTH: u64 = 0;
/// Maximum amount of bridges to use in a multi-hop swap
pub const BRIDGES_MAX_DEPTH: u64 = 2;
/// Swap execution depth limit
pub const BRIDGES_EXECUTION_MAX_DEPTH: u64 = 3;

pub fn try_build_swap_msg(
    deps: Deps,
    config: &Config,
    from: AssetInfo,
    to: AssetInfo,
    amount_in: Uint128,
) -> Result<CosmosMsg, ContractError> {
    let pool = get_pool(deps, config, from.clone(), to)?;
    let msg = build_swap_msg(config, pool, from, amount_in)?;
    Ok(msg)
}

pub fn build_swap_msg(
    config: &Config,
    pool: PairInfo,
    from: AssetInfo,
    amount_in: Uint128,
) -> Result<CosmosMsg, ContractError> {
    if from.is_native_token() {
        let mut offer_asset = Asset {
            info: from.clone(),
            amount: amount_in,
        };

        offer_asset.amount = amount_in;

        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pool.contract_addr.to_string(),
            msg: to_binary(&astroport::pair::ExecuteMsg::Swap {
                offer_asset,
                belief_price: None,
                max_spread: Some(config.max_spread),
                to: None,
            })?,
            funds: vec![Coin {
                denom: from.to_string(),
                amount: amount_in,
            }],
        }))
    } else {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: from.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: pool.contract_addr.to_string(),
                amount: amount_in,
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: Some(config.max_spread),
                    to: None,
                })?,
            })?,
            funds: vec![],
        }))
    }
}

pub fn build_swap_bridge_msg(
    env: Env,
    bridge_assets: Vec<AssetInfo>,
    depth: u64,
) -> StdResult<CosmosMsg> {
    let msg: CosmosMsg =
        // Swap bridge assets
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::SwapBridgeAssets {
                assets: bridge_assets,
                depth,
            })?,
            funds: vec![],
        });

    Ok(msg)
}

pub fn validate_bridge(
    deps: Deps,
    config: &Config,
    from_token: AssetInfo,
    bridge_token: AssetInfo,
    stablecoin_token: AssetInfo,
    depth: u64,
) -> Result<PairInfo, ContractError> {
    // Check if the bridge pool exists
    let bridge_pool = get_pool(deps, config, from_token.clone(), bridge_token.clone())?;

    // Check if the bridge token - stablecoin pool exists
    let stablecoin_pool = get_pool(deps, config, bridge_token.clone(), stablecoin_token.clone());
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
            config,
            bridge_token,
            next_bridge_token,
            stablecoin_token,
            depth + 1,
        )?;
    }

    Ok(bridge_pool)
}

pub fn get_pool(
    deps: Deps,
    config: &Config,
    from: AssetInfo,
    to: AssetInfo,
) -> Result<PairInfo, ContractError> {
    query_pair_info(
        &deps.querier,
        config.factory_contract.clone(),
        &[from.clone(), to.clone()],
    )
    .map_err(|_| ContractError::InvalidBridgeNoPool(from.clone(), to.clone()))
}
