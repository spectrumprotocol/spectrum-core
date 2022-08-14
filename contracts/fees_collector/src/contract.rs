use crate::error::ContractError;
use crate::state::{Config, BRIDGES, CONFIG, OWNERSHIP_PROPOSAL};

use crate::utils::{
    build_swap_bridge_msg, try_build_swap_msg, validate_bridge, BRIDGES_EXECUTION_MAX_DEPTH,
    BRIDGES_INITIAL_DEPTH,
};
use astroport::asset::{addr_validate_to_lower, native_asset_info, Asset, AssetInfo, ULUNA_DENOM, AssetInfoExt};

use astroport::common::{propose_new_owner, drop_ownership_proposal, claim_ownership};
use cosmwasm_std::{
    entry_point, to_binary, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo,
    Order, Response, StdError, StdResult, Uint128, WasmMsg, attr,
};
use cw2::set_contract_version;
use spectrum::fees_collector::{
    AssetWithLimit, BalancesResponse, ExecuteMsg, InstantiateMsg, MigrateMsg,
    QueryMsg,
};
use std::collections::{HashMap, HashSet};
use spectrum::adapters::asset::AssetEx;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "spectrum-fees-collector";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Sets the default maximum spread (as a percentage) used when swapping fee tokens to stablecoin.
const DEFAULT_MAX_SPREAD: u64 = 5; // 5%

/// ## Description
/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
/// Returns a default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the parameters used for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let max_spread = if let Some(max_spread) = msg.max_spread {
        if max_spread.gt(&Decimal::one()) {
            return Err(ContractError::IncorrectMaxSpread {});
        };
        max_spread
    } else {
        Decimal::percent(DEFAULT_MAX_SPREAD)
    };

    msg.stablecoin.check(deps.api)?;

    let config = Config {
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        operator: addr_validate_to_lower(deps.api, &msg.operator)?,
        factory_contract: addr_validate_to_lower(deps.api, &msg.factory_contract)?,
        stablecoin: msg.stablecoin,
        target_list: msg.target_list.into_iter()
                                .map(|(addr, weight)| (addr_validate_to_lower(deps.api, addr).unwrap(), weight))
                                .collect(),
        max_spread,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}

/// ## Description
/// Exposes execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::Collect { assets }** Swaps collected fee tokens to stablecoin
/// and distributes the stablecoin to beneficiary.
///
/// * **ExecuteMsg::UpdateConfig {
///             factory_contract,
///             staking_contract,
///             governance_contract,
///             governance_percent,
///             max_spread,
///         }** Updates general contract settings stores in the [`Config`].
///
/// * **ExecuteMsg::UpdateBridges { add, remove }** Adds or removes bridge assets used to swap fee tokens to stablecoin.
///
/// * **ExecuteMsg::SwapBridgeAssets { assets }** Swap fee tokens (through bridges) to stablecoin.
///
/// * **ExecuteMsg::DistributeFees {}** Private method used by the contract to distribute fees rewards.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Collect { assets } => collect(deps, env, assets),
        ExecuteMsg::UpdateBridges { add, remove } => update_bridges(deps, info, add, remove),
        ExecuteMsg::UpdateConfig {
            operator,
            factory_contract,
            target_list,
            max_spread,
        } => update_config(
            deps,
            info,
            operator,
            factory_contract,
            target_list,
            max_spread,
        ),
        ExecuteMsg::SwapBridgeAssets { assets, depth } => {
            swap_bridge_assets(deps, env, info, assets, depth)
        }
        ExecuteMsg::DistributeFees {} => distribute_fees(deps, env, info),
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
/// Swaps fee tokens to stablecoin and distribute the resulting stablecoin to beneficiary.
/// Returns a [`ContractError`] on failure, otherwise returns a [`Response`] object if the
/// operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **assets** is a vector that contains objects of type [`AssetWithLimit`]. These are the fee tokens being swapped to stablecoin.
fn collect(
    deps: DepsMut,
    env: Env,
    assets: Vec<AssetWithLimit>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let stablecoin = config.stablecoin.clone();

    // Check for duplicate assets
    let mut uniq = HashSet::new();
    if !assets
        .clone()
        .into_iter()
        .all(|a| uniq.insert(a.info.to_string()))
    {
        return Err(ContractError::DuplicatedAsset {});
    }
    let response = Response::default();
    // Swap all non stablecoin tokens
    let (mut messages, bridge_assets) = swap_assets(
        deps.as_ref(),
        env.clone(),
        &config,
        assets
            .into_iter()
            .filter(|a| a.info.ne(&stablecoin))
            .collect(),
    )?;

    // If no swap messages - send stablecoin directly to beneficiary
    if !messages.is_empty() && !bridge_assets.is_empty() {
        messages.push(build_swap_bridge_msg(
            env.clone(),
            bridge_assets,
            BRIDGES_INITIAL_DEPTH,
        )?);
    }

    let distribute_fee = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&ExecuteMsg::DistributeFees {})?,
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
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **cfg** is an object of type [`Config`]. This is the Maker contract configuration.
///
/// * **assets** is a vector that contains objects of type [`AssetWithLimit`]. These are the assets to swap to stablecoin.
///
/// * **with_validation** is a parameter of type [`u64`]. Determines whether the swap operation is validated or not.
fn swap_assets(
    deps: Deps,
    env: Env,
    config: &Config,
    assets: Vec<AssetWithLimit>,
) -> Result<(Vec<CosmosMsg>, Vec<AssetInfo>), ContractError> {
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut bridge_assets = HashMap::new();

    for a in assets {
        // Get balance
        let mut balance = a
            .info
            .query_pool(&deps.querier, env.contract.address.clone())?;
        if let Some(limit) = a.limit {
            if limit < balance && limit > Uint128::zero() {
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
/// of type [`SubMsg`] if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **cfg** is an object of type [`Config`]. This is the Maker contract configuration.
///
/// * **from_token** is an object of type [`AssetInfo`]. This is the token to swap to stablecoin.
///
/// * **amount_in** is an object of type [`Uint128`]. This is the amount of fee tokens to swap.
fn swap(
    deps: Deps,
    config: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<SwapTarget, ContractError> {
    let stablecoin = config.stablecoin.clone();
    let uluna = native_asset_info(ULUNA_DENOM.to_string());

    // 2. Check if bridge tokens exist
    let bridge_token = BRIDGES.load(deps.storage, from_token.to_string());
    if let Ok(asset) = bridge_token {
        let msg = try_build_swap_msg(&deps.querier, config, from_token, asset.clone(), amount_in)?;
        return Ok(SwapTarget::Bridge { asset, msg });
    }

    // 4. Check for a pair with LUNA
    if from_token.ne(&uluna) {
        let swap_to_uluna =
            try_build_swap_msg(&deps.querier, config, from_token.clone(), uluna.clone(), amount_in);
        if let Ok(msg) = swap_to_uluna {
            return Ok(SwapTarget::Bridge { asset: uluna, msg });
        }
    }

    // 5. Check for a direct pair with stablecoin
    let swap_to_stablecoin =
        try_build_swap_msg(&deps.querier, config, from_token.clone(), stablecoin, amount_in);
    if let Ok(msg) = swap_to_stablecoin {
        return Ok(SwapTarget::Stable(msg));
    }

    Err(ContractError::CannotSwap(from_token))
}

/// ## Description
/// Swaps collected fees using bridge assets. Returns a [`ContractError`] on failure.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **assets** is a vector field of type [`AssetWithLimit`]. These are the fee tokens to swap as well as amounts of tokens to swap.
///
/// * **depth** is an object of type [`u64`]. This is the maximum route length used to swap a fee token.
///
/// ##Executor
/// Only the Maker contract itself can execute this.
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

    let (mut messages, bridge_assets) = swap_assets(deps.as_ref(), env.clone(), &config, bridges)?;

    // There should always be some messages, if there are none - something went wrong
    if messages.is_empty() {
        return Err(ContractError::Std(StdError::generic_err(
            "Empty swap messages",
        )));
    }

    if !bridge_assets.is_empty() {
        messages.push(build_swap_bridge_msg(env, bridge_assets, depth + 1)?)
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "swap_bridge_assets"))
}

/// ## Description
/// Distributes stablecoin rewards to beneficiary. Returns a [`ContractError`] on failure.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// ##Executor
/// Only the Maker contract itself can execute this.
fn distribute_fees(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    let config = CONFIG.load(deps.storage)?;
    let (distribute_msg, attributes) = distribute(deps, env, &config)?;

    Ok(Response::new()
        .add_messages(distribute_msg)
        .add_attributes(attributes))
}

type DistributeMsgParts = (Vec<CosmosMsg>, Vec<(String, String)>);

/// ## Description
/// Private function that performs the stablecoin token distribution to beneficiary. Returns a [`ContractError`] on failure,
/// otherwise returns a vector that contains the objects of type [`SubMsg`] if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **cfg** is an object of type [`Config`].
fn distribute(
    deps: DepsMut,
    env: Env,
    config: &Config,
) -> Result<DistributeMsgParts, ContractError> {
    let mut messages = vec![];
    let mut attributes = vec![];

    let stablecoin = config.stablecoin.clone();

    let total_weight = config.target_list.iter()
        .map(|(_, weight)| *weight)
        .sum::<u64>();

    let total_amount = stablecoin.query_pool(&deps.querier, env.contract.address)?;

    if total_amount.is_zero() {
        return Ok((messages, attributes));
    }

    for (to, weight) in &config.target_list {
        let amount = total_amount.multiply_ratio(*weight, total_weight);
        if !amount.is_zero() {
            let send_msg = stablecoin.with_balance(amount).transfer_msg(to)?;
            messages.push(send_msg);
            attributes.push(("to".to_string(), to.to_string()));
            attributes.push(("amount".to_string(), amount.to_string()));
        }
    }

    attributes.push(("action".to_string(), "distribute_fees".to_string()));

    Ok((messages, attributes))
}

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    operator: Option<String>,
    factory_contract: Option<String>,
    target_list: Option<Vec<(String, u64)>>,
    max_spread: Option<Decimal>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(operator) = operator {
        config.operator = addr_validate_to_lower(deps.api, &operator)?;
    }

    if let Some(factory_contract) = factory_contract {
        config.factory_contract = addr_validate_to_lower(deps.api, &factory_contract)?;
    }

    if let Some(max_spread) = max_spread {
        if max_spread.gt(&Decimal::one()) {
            return Err(ContractError::IncorrectMaxSpread {});
        };
        config.max_spread = max_spread;
    }

    if let Some(target_list) = target_list {
        config.target_list = target_list.into_iter()
        .map(|(addr, weight)| (addr_validate_to_lower(deps.api, addr).unwrap(), weight))
        .collect()
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

/// ## Description
/// Adds or removes bridge tokens used to swap fee tokens to stablecoin. Returns a [`ContractError`] on failure.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **add** is an [`Option`] field of type [`Vec<(AssetInfo, AssetInfo)>`]. This is a vector of bridge tokens added to swap fee tokens with.
///
/// * **remove** is an [`Option`] field of type [`Vec<AssetInfo>`]. This is a vector of bridge
/// tokens removed from being used to swap certain fee tokens.
///
/// ##Executor
/// Only the operator can execute this.
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
    let stablecoin = config.stablecoin.clone();
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
        let asset = match addr_validate_to_lower(deps.api, &asset_label) {
            Ok(contract_addr) => AssetInfo::Token { contract_addr },
            Err(_) => AssetInfo::NativeToken { denom: asset_label },
        };
        // Check that bridge tokens can be swapped to stablecoin
        validate_bridge(
            deps.as_ref(),
            config.factory_contract.clone(),
            asset,
            bridge.clone(),
            stablecoin.clone(),
            BRIDGES_INITIAL_DEPTH,
        )?;
    }

    Ok(Response::default().add_attribute("action", "update_bridges"))
}

/// ## Description
/// Exposes all the queries available in the contract.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the Maker contract configuration using a [`ConfigResponse`] object.
///
/// * **QueryMsg::Balances { assets }** Returns the balances of certain fee tokens accrued by the Maker
/// using a [`ConfigResponse`] object.
///
/// * **QueryMsg::Bridges {}** Returns the bridges used for swapping fee tokens
/// using a vector of [`(String, String)`] denoting Asset -> Bridge connections.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Balances { assets } => to_binary(&query_get_balances(deps, env, assets)?),
        QueryMsg::Bridges {} => to_binary(&query_bridges(deps, env)?),
    }
}

/// ## Description
/// Returns Maker's fee token balances for specific tokens using a [`ConfigResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **assets** is a vector that contains objects of type [`AssetInfo`]. These are the assets for which we query the Maker's balances.
fn query_get_balances(deps: Deps, env: Env, assets: Vec<AssetInfo>) -> StdResult<BalancesResponse> {
    let mut resp = BalancesResponse { balances: vec![] };

    for a in assets {
        // Get balance
        let balance = a.query_pool(&deps.querier, env.contract.address.clone())?;
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
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
fn query_bridges(deps: Deps, _env: Env) -> StdResult<Vec<(String, String)>> {
    BRIDGES
        .range(deps.storage, None, None, Order::Ascending)
        .map(|bridge| {
            let (bridge, asset) = bridge?;
            Ok((bridge, asset.to_string()))
        })
        .collect()
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
