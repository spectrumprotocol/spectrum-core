use crate::error::ContractError;
use crate::state::{Config, BRIDGES, CONFIG, OWNERSHIP_PROPOSAL};

use crate::utils::{build_swap_bridge_msg, try_build_swap_msg, validate_bridge, BRIDGES_EXECUTION_MAX_DEPTH, BRIDGES_INITIAL_DEPTH, try_swap_simulation};
use astroport::asset::{native_asset_info, Asset, AssetInfo, ULUNA_DENOM, AssetInfoExt};

use astroport::common::{propose_new_owner, drop_ownership_proposal, claim_ownership};
use cosmwasm_std::{entry_point, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult, Uint128, WasmMsg, attr, Addr};
use spectrum::fees_collector::{AssetWithLimit, BalancesResponse, CollectSimulationResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use std::collections::{HashMap, HashSet};
use spectrum::adapters::asset::AssetEx;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {

    msg.stablecoin.check(deps.api)?;

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        operator: deps.api.addr_validate(&msg.operator)?,
        factory_contract: deps.api.addr_validate(&msg.factory_contract)?,
        stablecoin: msg.stablecoin,
        target_list: msg.target_list.into_iter()
                                .map(|(addr, weight)| Ok((deps.api.addr_validate(&addr)?, weight)))
                                .collect::<StdResult<_>>()?,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// ## Description
/// Exposes execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Collect { assets, minimum_receive } => collect(deps, env, info, assets, minimum_receive),
        ExecuteMsg::UpdateBridges { add, remove } => update_bridges(deps, info, add, remove),
        ExecuteMsg::UpdateConfig {
            operator,
            factory_contract,
            target_list,
        } => update_config(
            deps,
            info,
            operator,
            factory_contract,
            target_list,
        ),
        ExecuteMsg::SwapBridgeAssets { assets, depth } => {
            swap_bridge_assets(deps, env, info, assets, depth)
        }
        ExecuteMsg::DistributeFees { minimum_receive } => distribute_fees(deps, env, info, minimum_receive),
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
        },
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(|e| e.into())
        },
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(|e| e.into())
        },
    }
}

/// ## Description
/// Swaps fee tokens to stablecoin and distribute the resulting stablecoin to the target list.
/// Returns a [`ContractError`] on failure, otherwise returns a [`Response`] object if the
/// operation was successful.
fn collect(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<AssetWithLimit>,
    minimum_receive: Option<Uint128>,
) -> Result<Response, ContractError> {

    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.operator {
        return Err(ContractError::Unauthorized {});
    }

    // Check for duplicate assets
    let mut uniq = HashSet::new();
    if !assets
        .iter()
        .all(|a| uniq.insert(a.info.to_string()))
    {
        return Err(ContractError::DuplicatedAsset {});
    }
    let response = Response::default();
    // Swap all non stablecoin tokens
    let (mut messages, bridge_assets) = swap_assets(
        deps.as_ref(),
        &env.contract.address,
        &config,
        assets
            .into_iter()
            .filter(|a| a.info.ne(&config.stablecoin))
            .collect(),
    )?;

    // If no swap messages - send stablecoin directly to beneficiary
    if !messages.is_empty() && !bridge_assets.is_empty() {
        messages.push(build_swap_bridge_msg(
            &env.contract.address,
            bridge_assets,
            BRIDGES_INITIAL_DEPTH,
        )?);
    }

    let distribute_fee = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::DistributeFees {
            minimum_receive,
        })?,
        funds: vec![],
    });
    messages.push(distribute_fee);

    Ok(response
        .add_messages(messages)
        .add_attribute("action", "collect"))
}

/// ## Description
/// This enum describes available token types that can be used as a SwapTarget.
enum SwapTarget {
    Stable(CosmosMsg),
    Bridge { asset: AssetInfo, msg: CosmosMsg },
}

/// ## Description
/// Swap all non stablecoin tokens to stablecoin. Returns a [`ContractError`] on failure, otherwise returns
/// a [`Response`] object if the operation was successful.
fn swap_assets(
    deps: Deps,
    contract_addr: &Addr,
    config: &Config,
    assets: Vec<AssetWithLimit>,
) -> Result<(Vec<CosmosMsg>, Vec<AssetInfo>), ContractError> {
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut bridge_assets = HashMap::new();

    for a in assets {
        // Get balance
        let mut balance = a.info.query_pool(&deps.querier, contract_addr)?;
        if let Some(limit) = a.limit {
            if limit < balance {
                balance = limit;
            }
        }

        if !balance.is_zero() {
            let swap_msg = swap(deps, config, a.info, balance)?;
            match swap_msg {
                SwapTarget::Stable(msg) => {
                    messages.push(msg);
                }
                SwapTarget::Bridge { asset, msg } => {
                    messages.push(msg);
                    bridge_assets.insert(asset.to_string(), asset);
                }
            }
        }
    }

    Ok((messages, bridge_assets.into_values().collect()))
}

/// ## Description
/// Checks if all required pools and bridges exists and performs a swap operation to stablecoin.
/// Returns a [`ContractError`] on failure, otherwise returns a vector that contains objects
/// of type [`SwapTarget`] if the operation was successful.
fn swap(
    deps: Deps,
    config: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<SwapTarget, ContractError> {
    let stablecoin = config.stablecoin.clone();
    let uluna = native_asset_info(ULUNA_DENOM.to_string());

    // Check if bridge tokens exist
    let bridge_token = BRIDGES.load(deps.storage, from_token.to_string());
    if let Ok(asset) = bridge_token {
        let msg = try_build_swap_msg(&deps.querier, config, from_token, asset.clone(), amount_in)?;
        return Ok(SwapTarget::Bridge { asset, msg });
    }

    // Check for a direct pair with stablecoin
    let swap_to_stablecoin =
        try_build_swap_msg(&deps.querier, config, from_token.clone(), stablecoin, amount_in);
    if let Ok(msg) = swap_to_stablecoin {
        return Ok(SwapTarget::Stable(msg));
    }

    // Check for a pair with LUNA
    if from_token.ne(&uluna) {
        let swap_to_uluna =
            try_build_swap_msg(&deps.querier, config, from_token.clone(), uluna.clone(), amount_in);
        if let Ok(msg) = swap_to_uluna {
            return Ok(SwapTarget::Bridge { asset: uluna, msg });
        }
    }

    Err(ContractError::CannotSwap(from_token))
}

/// ## Description
/// Swaps collected fees using bridge assets. Returns a [`ContractError`] on failure.
fn swap_bridge_assets(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<AssetInfo>,
    depth: u64,
) -> Result<Response, ContractError> {
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    if assets.is_empty() {
        return Ok(Response::default());
    }

    // Check that the contract doesn't call itself endlessly
    if depth >= BRIDGES_EXECUTION_MAX_DEPTH {
        return Err(ContractError::MaxBridgeDepth(depth));
    }

    let config = CONFIG.load(deps.storage)?;

    let bridges = assets
        .into_iter()
        .map(|a| AssetWithLimit {
            info: a,
            limit: None,
        })
        .collect();

    let (mut messages, bridge_assets) = swap_assets(
        deps.as_ref(),
        &env.contract.address,
        &config,
        bridges)?;

    // There should always be some messages, if there are none - something went wrong
    if messages.is_empty() {
        return Err(ContractError::Std(StdError::generic_err(
            "Empty swap messages",
        )));
    }

    if !bridge_assets.is_empty() {
        messages.push(build_swap_bridge_msg(&env.contract.address, bridge_assets, depth + 1)?)
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "swap_bridge_assets"))
}

/// ## Description
/// Distributes stablecoin rewards to the target list. Returns a [`ContractError`] on failure.
fn distribute_fees(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    minimum_receive: Option<Uint128>,
) -> Result<Response, ContractError> {

    // Only the contract itself can call this function
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    let config = CONFIG.load(deps.storage)?;
    let (distribute_msg, attributes) = distribute(deps, env, &config, minimum_receive)?;

    Ok(Response::new()
        .add_messages(distribute_msg)
        .add_attributes(attributes))
}

type DistributeMsgParts = (Vec<CosmosMsg>, Vec<(String, String)>);

/// ## Description
/// Private function that performs the stablecoin token distribution to beneficiary. Returns a [`ContractError`] on failure,
/// otherwise returns a vector that contains the objects of type [`CosmosMsg`] if the operation was successful.
fn distribute(
    deps: DepsMut,
    env: Env,
    config: &Config,
    minimum_receive: Option<Uint128>,
) -> Result<DistributeMsgParts, ContractError> {
    let mut messages = vec![];
    let mut attributes = vec![];

    let total_amount = config.stablecoin.query_pool(&deps.querier, &env.contract.address)?;
    if let Some(minimum_receive) = minimum_receive {
        if total_amount < minimum_receive {
            return Err(ContractError::AssertionMinimumReceive {
                minimum_receive,
                amount: total_amount,
            });
        }
    }

    if total_amount.is_zero() {
        return Ok((messages, attributes));
    }

    let total_weight = config.target_list.iter()
        .map(|(_, weight)| *weight)
        .sum::<u64>();

    for (to, weight) in &config.target_list {
        let amount = total_amount.multiply_ratio(*weight, total_weight);
        if !amount.is_zero() {
            let send_msg = config.stablecoin.with_balance(amount).transfer_msg(to)?;
            messages.push(send_msg);
            attributes.push(("to".to_string(), to.to_string()));
            attributes.push(("amount".to_string(), amount.to_string()));
        }
    }

    attributes.push(("action".to_string(), "distribute_fees".to_string()));

    Ok((messages, attributes))
}

/// ## Description
/// Updates contract config. Returns a [`ContractError`] on failure or the [`CONFIG`] data will be updated.
#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    operator: Option<String>,
    factory_contract: Option<String>,
    target_list: Option<Vec<(String, u64)>>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(operator) = operator {
        config.operator = deps.api.addr_validate(&operator)?;
    }

    if let Some(factory_contract) = factory_contract {
        config.factory_contract = deps.api.addr_validate(&factory_contract)?;
    }

    if let Some(target_list) = target_list {
        config.target_list = target_list.into_iter()
        .map(|(addr, weight)| Ok((deps.api.addr_validate(&addr)?, weight)))
        .collect::<StdResult<_>>()?
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

/// ## Description
/// Adds or removes bridge tokens used to swap fee tokens to stablecoin. Returns a [`ContractError`] on failure.
fn update_bridges(
    deps: DepsMut,
    info: MessageInfo,
    add: Option<Vec<(AssetInfo, AssetInfo)>>,
    remove: Option<Vec<AssetInfo>>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.operator {
        return Err(ContractError::Unauthorized {});
    }

    // Remove old bridges
    if let Some(remove_bridges) = remove {
        for asset in remove_bridges {
            BRIDGES.remove(deps.storage, asset.to_string());
        }
    }

    // Add new bridges
    if let Some(add_bridges) = add {
        for (asset, bridge) in add_bridges {
            if asset.equal(&bridge) {
                return Err(ContractError::InvalidBridge(asset, bridge));
            }
            BRIDGES.save(deps.storage, asset.to_string(), &bridge)?;
        }
    }

    let bridges = BRIDGES
        .range(deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<(String, AssetInfo)>>>()?;

    for (asset_label, bridge) in bridges {
        let asset = match deps.api.addr_validate(&asset_label) {
            Ok(contract_addr) => AssetInfo::Token { contract_addr },
            Err(_) => AssetInfo::NativeToken { denom: asset_label },
        };
        // Check that bridge tokens can be swapped to stablecoin
        validate_bridge(
            deps.as_ref(),
            &config.factory_contract,
            &asset,
            &bridge,
            &config.stablecoin,
            BRIDGES_INITIAL_DEPTH,
        )?;
    }

    Ok(Response::default().add_attribute("action", "update_bridges"))
}

/// ## Description
/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Balances { assets } => to_binary(&query_get_balances(deps, env, assets)?),
        QueryMsg::Bridges {} => to_binary(&query_bridges(deps, env)?),
        QueryMsg::CollectSimulation { assets } => to_binary(&query_collect_simulation(deps, env, assets)?),
    }
}

/// ## Description
/// Returns token balances for specific tokens using a [`ConfigResponse`] object.
fn query_get_balances(deps: Deps, env: Env, assets: Vec<AssetInfo>) -> StdResult<BalancesResponse> {
    let mut resp = BalancesResponse { balances: vec![] };

    for a in assets {
        // Get balance
        let balance = a.query_pool(&deps.querier, &env.contract.address)?;
        if !balance.is_zero() {
            resp.balances.push(Asset {
                info: a,
                amount: balance,
            })
        }
    }

    Ok(resp)
}

/// ## Description
/// Returns bridge tokens used for swapping fee tokens to stablecoin.
fn query_bridges(deps: Deps, _env: Env) -> StdResult<Vec<(String, String)>> {
    BRIDGES
        .range(deps.storage, None, None, Order::Ascending)
        .map(|bridge| {
            let (bridge, asset) = bridge?;
            Ok((bridge, asset.to_string()))
        })
        .collect()
}

fn query_collect_simulation(
    deps: Deps,
    env: Env,
    assets: Vec<AssetWithLimit>
) -> Result<CollectSimulationResponse, ContractError> {

    // Check for duplicate assets
    let mut uniq = HashMap::new();
    for a in assets {

        // query balance
        let mut balance = a.info.query_pool(&deps.querier, &env.contract.address)?;
        if let Some(limit) = a.limit {
            if limit < balance {
                balance = limit;
            }
        }

        // swap
        if uniq.insert(a.info, balance).is_some() {
            return Err(ContractError::DuplicatedAsset {});
        }
    }

    let config = CONFIG.load(deps.storage)?;
    if !uniq.contains_key(&config.stablecoin) {
        let stable_amount = config.stablecoin.query_pool(&deps.querier, &env.contract.address)?;
        uniq.insert(config.stablecoin.clone(), stable_amount);
    }

    bulk_swap_simulation(deps, uniq, config, BRIDGES_INITIAL_DEPTH)
}

fn bulk_swap_simulation(
    deps: Deps,
    assets: HashMap<AssetInfo, Uint128>,
    config: Config,
    depth: u64,
) -> Result<CollectSimulationResponse, ContractError> {

    let mut next_assets: HashMap<AssetInfo, Uint128> = HashMap::new();
    let uluna = native_asset_info(ULUNA_DENOM.to_string());
    for (from_asset_info, amount_in) in assets {

        if from_asset_info.eq(&config.stablecoin) {
            add_amount(&mut next_assets, config.stablecoin.clone(), amount_in);
            continue;
        }

        if amount_in.is_zero() {
            continue;
        }

        // Check if bridge tokens exist
        let bridge_token = BRIDGES.load(deps.storage, from_asset_info.to_string());
        if let Ok(to_asset_info) = bridge_token {
            let return_amount = try_swap_simulation(&deps.querier, &config, from_asset_info, to_asset_info.clone(), amount_in)?;
            add_amount(&mut next_assets, to_asset_info, return_amount);
            continue;
        }

        // Check for a direct pair with stablecoin
        let return_amount = try_swap_simulation(&deps.querier, &config, from_asset_info.clone(), config.stablecoin.clone(), amount_in);
        if let Ok(return_amount) = return_amount {
            add_amount(&mut next_assets, config.stablecoin.clone(), return_amount);
            continue;
        }

        // Check for a pair with LUNA
        if from_asset_info.ne(&uluna) {
            let return_amount = try_swap_simulation(&deps.querier, &config, from_asset_info.clone(), uluna.clone(), amount_in);
            if let Ok(return_amount) = return_amount {
                add_amount(&mut next_assets, uluna.clone(), return_amount);
                continue;
            }
        }

        return Err(ContractError::CannotSwap(from_asset_info));
    }

    // reduce until 1 item
    if next_assets.len() < 2 {
        return Ok(CollectSimulationResponse {
            return_amount: next_assets.get(&config.stablecoin)
                .copied()
                .unwrap_or_default(),
        });
    }

    let next_depth = depth + 1;
    if next_depth >= BRIDGES_EXECUTION_MAX_DEPTH {
        return Err(ContractError::MaxBridgeDepth(depth));
    }


    bulk_swap_simulation(deps, next_assets, config, next_depth)
}

fn add_amount(assets: &mut HashMap<AssetInfo, Uint128>, key: AssetInfo, return_amount: Uint128) {
    let prev_amount = assets.get(&key)
        .copied()
        .unwrap_or_default();
    assets.insert(key, return_amount + prev_amount);
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
