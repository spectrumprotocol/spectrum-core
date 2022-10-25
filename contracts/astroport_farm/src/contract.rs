use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, Uint128,
};

use crate::{
    bond::{bond, bond_assets, bond_to},
    compound::{compound, stake},
    error::ContractError,
    ownership::{claim_ownership, drop_ownership_proposal, propose_new_owner},
    state::{Config, State, CONFIG, OWNERSHIP_PROPOSAL},
};

use cw20::{Cw20ReceiveMsg, MarketingInfoResponse, MinterResponse};
use spectrum::adapters::generator::Generator;

use crate::bond::{query_reward_info, unbond};
use crate::state::STATE;
use spectrum::astroport_farm::{
    CallbackMsg, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
use spectrum::compound_proxy::Compounder;
use crate::cw20::{execute_burn, execute_burn_from, execute_decrease_allowance, execute_increase_allowance, execute_send, execute_send_from, execute_transfer, execute_transfer_from, query_all_accounts, query_all_allowances, query_allowance, query_balance, query_token_info};

/// ## Description
/// Validates that decimal value is in the range 0 to 1
fn validate_percentage(value: Decimal, field: &str) -> StdResult<()> {
    if value > Decimal::one() {
        Err(StdError::generic_err(field.to_string() + " must be 0 to 1"))
    } else {
        Ok(())
    }
}

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
    msg.validate()?;
    validate_percentage(msg.fee, "fee")?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            staking_contract: Generator(deps.api.addr_validate(&msg.staking_contract)?),
            compound_proxy: Compounder(deps.api.addr_validate(&msg.compound_proxy)?),
            controller: deps.api.addr_validate(&msg.controller)?,
            fee: msg.fee,
            fee_collector: deps.api.addr_validate(&msg.fee_collector)?,
            liquidity_token: deps.api.addr_validate(&msg.liquidity_token)?,
            base_reward_token: deps.api.addr_validate(&msg.base_reward_token)?,
            name: msg.name,
            symbol: msg.symbol,
        },
    )?;

    STATE.save(
        deps.storage,
        &State {
            total_bond_share: Uint128::zero(),
        },
    )?;

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
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::UpdateConfig {
            compound_proxy,
            controller,
            fee,
            fee_collector,
        } => update_config(deps, info, compound_proxy, controller, fee, fee_collector),
        ExecuteMsg::Unbond { amount } => unbond(deps, env, info, amount),
        ExecuteMsg::BondAssets {
            assets,
            minimum_receive,
            no_swap,
            slippage_tolerance,
        } => bond_assets(
            deps,
            env,
            info,
            assets,
            minimum_receive,
            no_swap,
            slippage_tolerance,
        ),
        ExecuteMsg::Compound {
            minimum_receive,
            slippage_tolerance,
        } => compound(deps, env, info, minimum_receive, slippage_tolerance),
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
        ExecuteMsg::Callback(msg) => handle_callback(deps, env, info, msg),

        // cw20
        ExecuteMsg::Transfer { recipient, amount } => execute_transfer(deps, env, info, recipient, amount),
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::Send { contract, amount, msg } => execute_send(deps, env, info, contract, amount, msg),
        ExecuteMsg::IncreaseAllowance { spender, amount, expires } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance { spender, amount, expires } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom { owner, recipient, amount } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::SendFrom { owner, contract, amount, msg } => execute_send_from(deps, env, info, owner, contract, amount, msg),
        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
        ExecuteMsg::Mint { .. } => Err(ContractError::Unauthorized {}),
        ExecuteMsg::UpdateMarketing { .. } => Err(ContractError::Unauthorized {}),
        ExecuteMsg::UploadLogo(_) => Err(ContractError::Unauthorized {}),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then a [`ContractError`] is returned,
/// otherwise returns a [`Response`] with the specified attributes if the operation was successful
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Bond { staker_addr }) => bond(
            deps,
            env,
            info,
            staker_addr.unwrap_or(cw20_msg.sender),
            cw20_msg.amount,
        ),
        Err(_) => Err(ContractError::InvalidMessage {}),
    }
}

/// ## Description
/// Updates contract config. Returns a [`ContractError`] on failure or the [`CONFIG`] data will be updated.
#[allow(clippy::too_many_arguments)]
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    compound_proxy: Option<String>,
    controller: Option<String>,
    fee: Option<Decimal>,
    fee_collector: Option<String>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(compound_proxy) = compound_proxy {
        config.compound_proxy = Compounder(deps.api.addr_validate(&compound_proxy)?);
    }

    if let Some(controller) = controller {
        config.controller = deps.api.addr_validate(&controller)?;
    }

    if let Some(fee) = fee {
        validate_percentage(fee, "fee")?;
        config.fee = fee;
    }

    if let Some(fee_collector) = fee_collector {
        config.fee_collector = deps.api.addr_validate(&fee_collector)?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

/// # Description
/// Handle the callbacks describes in the [`CallbackMsg`]. Returns an [`ContractError`] on failure, otherwise returns the [`Response`]
pub fn handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Callback functions can only be called by this contract itself
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }
    match msg {
        CallbackMsg::Stake {
            prev_balance,
            minimum_receive,
        } => stake(deps, env, info, prev_balance, minimum_receive),
        CallbackMsg::BondTo {
            to,
            prev_balance,
            minimum_receive,
        } => bond_to(deps, env, info, to, prev_balance, minimum_receive),
    }
}

/// ## Description
/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::RewardInfo { staker_addr } => {
            to_binary(&query_reward_info(deps, env, staker_addr)?)
        }
        QueryMsg::State {} => to_binary(&query_state(deps)?),

        // cw20
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::TokenInfo { } => to_binary(&query_token_info(deps)?),
        QueryMsg::Minter { } => to_binary::<Option<MinterResponse>>(&None),
        QueryMsg::Allowance { owner, spender } => to_binary(&query_allowance(deps, owner, spender)?),
        QueryMsg::AllAllowances { owner, start_after, limit } => to_binary(&query_all_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => to_binary(&query_all_accounts(deps, start_after, limit)?),
        QueryMsg::MarketingInfo { } => to_binary(&MarketingInfoResponse::default()),
        QueryMsg::DownloadLogo { } => Err(StdError::not_found("logo")),
    }
}

/// ## Description
/// Returns contract config
fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

/// ## Description
/// Returns contract state
fn query_state(deps: Deps) -> StdResult<State> {
    let state = STATE.load(deps.storage)?;
    Ok(state)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    msg.validate()?;

    let mut config = CONFIG.load(deps.storage)?;
    config.name = msg.name;
    config.symbol = msg.symbol;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}
