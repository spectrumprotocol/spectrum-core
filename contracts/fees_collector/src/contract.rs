use crate::error::ContractError;
use crate::state::{Config, BRIDGES, CONFIG, OWNERSHIP_PROPOSAL};

use crate::utils::{
    build_distribute_msg, build_swap_msg, try_build_swap_msg, validate_bridge,
    BRIDGES_EXECUTION_MAX_DEPTH, BRIDGES_INITIAL_DEPTH,
};
use astroport::asset::{
    addr_validate_to_lower, native_asset_info, token_asset_info, Asset, AssetInfo,
    PairInfo, ULUNA_DENOM,
};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};

use astroport::pair::QueryMsg as PairQueryMsg;
use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Attribute, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, QueryRequest, Response, StdError, StdResult, SubMsg, Uint128,
    WasmMsg, WasmQuery,
};
use cw2::{set_contract_version};
use cw20::Cw20ExecuteMsg;
use spectrum::fees_collector::{InstantiateMsg, ExecuteMsg, QueryMsg, ConfigResponse, BalancesResponse, AssetWithLimit, MigrateMsg};
use std::collections::{HashMap, HashSet};

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

    let cfg = Config {
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        factory_contract: addr_validate_to_lower(deps.api, &msg.factory_contract)?,
        stablecoin_token_contract: addr_validate_to_lower(deps.api, &msg.stablecoin_token_contract)?,
        beneficiary: addr_validate_to_lower(deps.api, &msg.beneficiary)?,
        max_spread,
    };

    CONFIG.save(deps.storage, &cfg)?;

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
        ExecuteMsg::UpdateConfig {
            factory_contract,
            beneficiary,
            max_spread,
        } => update_config(
            deps,
            info,
            factory_contract,
            beneficiary,
            max_spread,
        ),
        ExecuteMsg::UpdateBridges { add, remove } => update_bridges(deps, info, add, remove),
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
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(|e| e.into())
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(|e| e.into())
        }
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
    let mut cfg = CONFIG.load(deps.storage)?;

    let stablecoin = token_asset_info(cfg.stablecoin_token_contract.clone());

    // Check for duplicate assets
    let mut uniq = HashSet::new();
    if !assets
        .clone()
        .into_iter()
        .all(|a| uniq.insert(a.info.to_string()))
    {
        return Err(ContractError::DuplicatedAsset {});
    }

    // Swap all non stablecoin tokens
    let (mut response, bridge_assets) = swap_assets(
        deps.as_ref(),
        env.clone(),
        &cfg,
        assets.into_iter().filter(|a| a.info.ne(&stablecoin)).collect(),
        true,
    )?;

    // If no swap messages - send stablecoin directly to beneficiary
    if response.messages.is_empty() {
        let (mut distribute_msg, attributes) = distribute(deps, env, &mut cfg)?;
        if !distribute_msg.is_empty() {
            response.messages.append(&mut distribute_msg);
            response = response.add_attributes(attributes);
        }
    } else {
        response.messages.push(build_distribute_msg(
            env,
            bridge_assets,
            BRIDGES_INITIAL_DEPTH,
        )?);
    }

    Ok(response.add_attribute("action", "collect"))
}

/// ## Description
/// This enum describes available token types that can be used as a SwapTarget.
enum SwapTarget {
    Stable(SubMsg),
    Bridge { asset: AssetInfo, msg: SubMsg },
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
    cfg: &Config,
    assets: Vec<AssetWithLimit>,
    with_validation: bool,
) -> Result<(Response, Vec<AssetInfo>), ContractError> {
    let mut response = Response::default();
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
            let swap_msg = if with_validation {
                swap(deps, cfg, a.info, balance)?
            } else {
                swap_no_validate(deps, cfg, a.info, balance)?
            };

            match swap_msg {
                SwapTarget::Stable(msg) => {
                    response.messages.push(msg);
                }
                SwapTarget::Bridge { asset, msg } => {
                    response.messages.push(msg);
                    bridge_assets.insert(asset.to_string(), asset);
                }
            }
        }
    }

    Ok((response, bridge_assets.into_values().collect()))
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
    cfg: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<SwapTarget, ContractError> {
    let stablecoin = token_asset_info(cfg.stablecoin_token_contract.clone());
    let uluna = native_asset_info(ULUNA_DENOM.to_string());

    // 2. Check if bridge tokens exist
    let bridge_token = BRIDGES.load(deps.storage, from_token.to_string());
    if let Ok(asset) = bridge_token {
        let bridge_pool = validate_bridge(
            deps,
            cfg,
            from_token.clone(),
            asset.clone(),
            stablecoin,
            BRIDGES_INITIAL_DEPTH,
        )?;

        let msg = build_swap_msg(deps, cfg, bridge_pool, from_token, amount_in)?;
        return Ok(SwapTarget::Bridge { asset, msg });
    }

    // 4. Check for a pair with LUNA
    if from_token.ne(&uluna) {
        let swap_to_uluna =
            try_build_swap_msg(deps, cfg, from_token.clone(), uluna.clone(), amount_in);
        if let Ok(msg) = swap_to_uluna {
            return Ok(SwapTarget::Bridge { asset: uluna, msg });
        }
    }

    // 5. Check for a direct pair with stablecoin
    let swap_to_stablecoin = try_build_swap_msg(deps, cfg, from_token.clone(), stablecoin, amount_in);
    if let Ok(msg) = swap_to_stablecoin {
        return Ok(SwapTarget::Stable(msg));
    }

    Err(ContractError::CannotSwap(from_token))
}

/// ## Description
/// Performs a swap operation to stablecoin without additional checks. Returns a [`ContractError`] on failure,
/// otherwise returns a vector that contains objects of type [`SubMsg`] if the operation
/// was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **cfg** is an object of type [`Config`]. This is the Maker contract configuration.
///
/// * **from_token** is an object of type [`AssetInfo`]. This is the token to swap to stablecoin.
///
/// * **amount_in** is an object of type [`Uint128`]. This is the amount of tokens to swap.
fn swap_no_validate(
    deps: Deps,
    cfg: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<SwapTarget, ContractError> {
    let stablecoin = token_asset_info(cfg.stablecoin_token_contract.clone());

    // Check if next level bridge exists
    let bridge_token = BRIDGES.load(deps.storage, from_token.to_string());
    if let Ok(asset) = bridge_token {
        let msg = try_build_swap_msg(deps, cfg, from_token, asset.clone(), amount_in)?;
        return Ok(SwapTarget::Bridge { asset, msg });
    }

    // Check for a direct swap to stablecoin
    let swap_to_stablecoin = try_build_swap_msg(deps, cfg, from_token.clone(), stablecoin, amount_in);
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

    let cfg = CONFIG.load(deps.storage)?;

    let bridges = assets
        .into_iter()
        .map(|a| AssetWithLimit {
            info: a,
            limit: None,
        })
        .collect();

    let (response, bridge_assets) = swap_assets(deps.as_ref(), env.clone(), &cfg, bridges, false)?;

    // There should always be some messages, if there are none - something went wrong
    if response.messages.is_empty() {
        return Err(ContractError::Std(StdError::generic_err(
            "Empty swap messages",
        )));
    }

    Ok(response
        .add_submessage(build_distribute_msg(env, bridge_assets, depth + 1)?)
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

    let mut cfg = CONFIG.load(deps.storage)?;
    let (distribute_msg, attributes) = distribute(deps, env, &mut cfg)?;
    if distribute_msg.is_empty() {
        return Ok(Response::default());
    }

    Ok(Response::default()
        .add_submessages(distribute_msg)
        .add_attributes(attributes))
}

type DistributeMsgParts = (Vec<SubMsg>, Vec<(String, String)>);

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
    cfg: &mut Config,
) -> Result<DistributeMsgParts, ContractError> {
    let mut result = vec![];
    let mut attributes = vec![];

    let stablecoin = token_asset_info(cfg.stablecoin_token_contract.clone());

    let amount = stablecoin.query_pool(&deps.querier, env.contract.address.clone())?;
    if amount.is_zero() {
        return Ok((result, attributes));
    } else {
        let send_msg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.stablecoin_token_contract.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: cfg.beneficiary.to_string(),
                amount,
            })?,
            funds: vec![],
        });
        result.push(SubMsg::new(send_msg))
    }

    attributes.push(("action".to_string(), "distribute_fees".to_string()));
    attributes.push((
        "amount".to_string(),
        amount.to_string(),
    ));

    Ok((result, attributes))
}

/// ## Description
/// Updates general contarct parameters. Returns a [`ContractError`] on failure or the [`Config`]
/// data will be updated if the transaction is successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **factory_contract** is an [`Option`] field of type [`String`]. This is the address of the factory contract.
///
/// * **beneficiary** is an [`Option`] field of type [`String`]. This is the address of the beneficiary.
///
/// * **max_spread** is an [`Option`] field of type [`Decimal`]. This is the max spread used when swapping fee tokens to stablecoin.
///
/// ##Executor
/// Only the owner can execute this.
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    factory_contract: Option<String>,
    beneficiary: Option<String>,
    max_spread: Option<Decimal>,
) -> Result<Response, ContractError> {
    let mut attributes = vec![attr("action", "set_config")];

    let mut config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(factory_contract) = factory_contract {
        config.factory_contract = addr_validate_to_lower(deps.api, &factory_contract)?;
        attributes.push(Attribute::new("factory_contract", &factory_contract));
    };

    if let Some(beneficiary) = beneficiary {
        config.beneficiary = addr_validate_to_lower(deps.api, &beneficiary)?;
        attributes.push(Attribute::new("beneficiary", &beneficiary));
    };

    if let Some(max_spread) = max_spread {
        if max_spread.gt(&Decimal::one()) {
            return Err(ContractError::IncorrectMaxSpread {});
        };

        config.max_spread = max_spread;
        attributes.push(Attribute::new("max_spread", max_spread.to_string()));
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
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
/// Only the owner can execute this.
fn update_bridges(
    deps: DepsMut,
    info: MessageInfo,
    add: Option<Vec<(AssetInfo, AssetInfo)>>,
    remove: Option<Vec<AssetInfo>>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Remove old bridges
    if let Some(remove_bridges) = remove {
        for asset in remove_bridges {
            BRIDGES.remove(
                deps.storage,
                addr_validate_to_lower(deps.api, asset.to_string().as_str())?.to_string(),
            );
        }
    }

    // Add new bridges
    let stablecoin = token_asset_info(cfg.stablecoin_token_contract.clone());
    if let Some(add_bridges) = add {
        for (asset, bridge) in add_bridges {
            if asset.equal(&bridge) {
                return Err(ContractError::InvalidBridge(asset, bridge));
            }

            // Check that bridge tokens can be swapped to stablecoin
            validate_bridge(
                deps.as_ref(),
                &cfg,
                asset.clone(),
                bridge.clone(),
                stablecoin.clone(),
                BRIDGES_INITIAL_DEPTH,
            )?;

            BRIDGES.save(deps.storage, asset.to_string(), &bridge)?;
        }
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
        QueryMsg::Config {} => to_binary(&query_get_config(deps)?),
        QueryMsg::Balances { assets } => to_binary(&query_get_balances(deps, env, assets)?),
        QueryMsg::Bridges {} => to_binary(&query_bridges(deps, env)?),
    }
}

/// ## Description
/// Returns information about the Maker configuration using a [`ConfigResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
fn query_get_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner,
        factory_contract: config.factory_contract,
        stablecoin_token_contract: config.stablecoin_token_contract,
        beneficiary: config.beneficiary,
        max_spread: config.max_spread,
    })
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
/// Returns asset information for the specified pair.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **contract_addr** is an object of type [`Addr`]. This is an Astroport pair contract address.
pub fn query_pair(deps: Deps, contract_addr: Addr) -> StdResult<[AssetInfo; 2]> {
    let res: PairInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&PairQueryMsg::Pair {})?,
    }))?;

    Ok(res.asset_infos)
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
