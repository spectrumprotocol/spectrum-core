use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use std::collections::{HashSet};

use cosmwasm_std::{entry_point, from_binary, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Fraction, Uint128};
use spectrum::pair_proxy::{Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, MAX_ASSETS};

use astroport::asset::{addr_validate_to_lower, Asset, AssetInfo};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;
use astroport::querier::query_token_precision;
use spectrum::adapters::router::Router;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "spectrom-pair-proxy";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Validate swap assets
    let asset_len = msg.asset_infos.len();
    if asset_len == 0 {
        return Err(ContractError::MustProvideNAssets {});
    }
    if asset_len > MAX_ASSETS {
        return Err(ContractError::SwapLimitExceeded {});
    }

    let mut uniq = HashSet::new();
    for asset_info in msg.asset_infos.iter() {
        asset_info.check(deps.api)?;
        if !uniq.insert(asset_info.to_string()) {
            return Err(ContractError::DuplicatedAssets {});
        }
    }

    let offer_precision = if let Some(offer_precision) = msg.offer_precision {
        offer_precision
    } else {
        query_token_precision(&deps.querier, &msg.asset_infos[0])?
    };
    let ask_precision = if let Some(ask_precision) = msg.ask_precision {
        ask_precision
    } else {
        query_token_precision(&deps.querier, &msg.asset_infos[msg.asset_infos.len() - 1])?
    };
    let config = Config {
        asset_infos: msg.asset_infos,
        router: Router(addr_validate_to_lower(deps.api, msg.router.as_str())?),
        router_type: msg.router_type,
        offer_precision,
        ask_precision,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::UpdateConfig { params: Binary }** Not supported.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::ProvideLiquidity {
///             assets,
///             slippage_tolerance,
///             auto_stake,
///             receiver,
///         }** Provides liquidity with the specified input parameters.
///
/// * **ExecuteMsg::Swap {
///             offer_asset,
///             belief_price,
///             max_spread,
///             to,
///         }** Performs an swap operation with the specified parameters.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
        } => {
            offer_asset.info.check(deps.api)?;
            if !offer_asset.is_native_token() {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = if let Some(to_addr) = to {
                Some(addr_validate_to_lower(deps.api, &to_addr)?)
            } else {
                None
            };
            swap(
                deps,
                env,
                info.clone(),
                info.sender,
                offer_asset,
                belief_price,
                max_spread,
                to_addr,
            )
        }
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise returns the [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **cw20_msg** is the object of type [`Cw20ReceiveMsg`].
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let contract_addr = info.sender.clone();
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Swap {
            belief_price,
            max_spread,
            to,
        }) => {
            let to_addr = if let Some(to_addr) = to {
                Some(addr_validate_to_lower(deps.api, to_addr.as_str())?)
            } else {
                None
            };

            swap(
                deps,
                env,
                info,
                Addr::unchecked(cw20_msg.sender),
                Asset {
                    info: AssetInfo::Token { contract_addr },
                    amount: cw20_msg.amount,
                },
                belief_price,
                max_spread,
                to_addr,
            )
        }
        Err(err) => Err(ContractError::Std(err)),
    }
}

/// ## Description
/// Performs an swap operation with the specified parameters. CONTRACT - a user must do token approval.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **sender** is the object of type [`Addr`]. Sets the default recipient of the swap operation.
///
/// * **offer_asset** is the object of type [`Asset`]. Proposed asset for swapping.
///
/// * **belief_price** is the object of type [`Option<Decimal>`]. Used to calculate the maximum spread.
///
/// * **max_spread** is the object of type [`Option<Decimal>`]. Sets the maximum spread of the swap operation.
///
/// * **to** is the object of type [`Option<Addr>`]. Sets the recipient of the swap operation.
#[allow(clippy::too_many_arguments)]
pub fn swap(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    offer_asset.assert_sent_native_token_balance(&info)?;

    let config = CONFIG.load(deps.storage)?;

    let (operations, offer_precision, ask_precision) = if offer_asset.info.equal(&config.asset_infos[0]) {
        (
            config.router_type.create_swap_operations(&config.asset_infos)?,
            config.offer_precision,
            config.ask_precision,
        )
    } else if offer_asset.info.equal(&config.asset_infos[config.asset_infos.len() - 1]) {
        let asset_infos: Vec<AssetInfo> = config.asset_infos.into_iter().rev().collect();
        (
            config.router_type.create_swap_operations(&asset_infos)?,
            config.ask_precision,
            config.offer_precision,
        )
    } else {
        return Err(ContractError::InvalidAsset {});
    };

    let to = to.unwrap_or(sender);
    let minimum_receive = match (belief_price, max_spread) {
        (Some(belief_price), Some(max_spread)) => {
            let minimum_receive = compute_minimum_receive(offer_asset.amount, belief_price, max_spread, offer_precision, ask_precision);
            Some(minimum_receive)
        },
        (_, _) => None,
    };
    let message = config.router.execute_swap_operations_msg(
        offer_asset,
        operations,
        minimum_receive,
        Some(to),
        max_spread,
    )?;

    Ok(Response::new()
        .add_message(message)
        .add_attribute("action", "swap"))
}

fn compute_minimum_receive(
    offer_amount: Uint128,
    belief_price: Decimal,
    max_spread: Decimal,
    offer_precision: u8,
    ask_precision: u8,
) -> Uint128 {
    let dec_adj = Decimal::from_ratio(
        10u128.pow(ask_precision as u32),
        10u128.pow(offer_precision as u32));
    let micro_price = Decimal::from_ratio(
        dec_adj.numerator(),
        belief_price.numerator()
    );
    offer_amount * micro_price * (Decimal::one() - max_spread)
}

/// ## Description
/// Available the query messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Pair {}** Returns information about a pair in an object of type [`PairInfo`].
///
/// * **QueryMsg::Config {}** Returns information about the controls settings in a
/// [`ConfigResponse`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
    }
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimum_receive() {
        let min_receive = compute_minimum_receive(
            Uint128::from(1234_567u128),
            Decimal::permille(10000_000),
            Decimal::zero(),
            3,
            5,
        );
        assert_eq!(min_receive, Uint128::from(0_12345u128));

        let min_receive = compute_minimum_receive(
            Uint128::from(12_34567u128),
            Decimal::permille(0_001),
            Decimal::zero(),
            5,
            0,
        );
        assert_eq!(min_receive, Uint128::from(12345u128));
    }
}
