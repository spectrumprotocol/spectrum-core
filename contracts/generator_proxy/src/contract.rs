use astroport::common::{propose_new_owner, drop_ownership_proposal, claim_ownership};
use cosmwasm_std::{entry_point, DepsMut, Env, MessageInfo, Response, from_binary, Deps, Binary, to_binary, Empty, StdError};
use cw20::Cw20ReceiveMsg;
use astroport::asset::addr_validate_to_lower;
use astroport_governance::utils::get_period;
use spectrum::adapters::generator::Generator;
use crate::bond::{callback_after_bond_changed, callback_after_claimed, callback_claim_rewards, callback_deposit, callback_withdraw, execute_deposit, execute_withdraw, query_deposit, query_pending_token, execute_claim_rewards};
use crate::config::{execute_update_config, execute_update_parameters, query_config, validate_percentage};
use crate::error::ContractError;
use crate::model::{CallbackMsg, Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, State};
use crate::query::{query_pool_info, query_reward_info, query_state, query_user_info};
use crate::state::{CONFIG, STATE, OWNERSHIP_PROPOSAL};

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {

    validate_percentage(msg.staker_rate, "staker_rate")?;
    validate_percentage(msg.boost_fee, "boost_fee")?;

    let config = Config {
        generator: Generator(addr_validate_to_lower(deps.api, &msg.generator)?),
        // astro_gov: msg.astro_gov.check(deps.api)?,
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        controller: addr_validate_to_lower(deps.api, &msg.controller)?,
        astro_token: addr_validate_to_lower(deps.api, &msg.astro_token)?,
        fee_collector: addr_validate_to_lower(deps.api, &msg.fee_collector)?,
        max_quota: msg.max_quota,
        staker_rate: msg.staker_rate,
        boost_fee: msg.boost_fee,
    };
    CONFIG.save(deps.storage, &config)?;

    let state = State {
        next_claim_period: get_period(env.block.time.seconds())?,
    };
    STATE.save(deps.storage, &state)?;

    Ok(Response::default())
}

/// ## Description
/// Exposes execute functions available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(cw20_msg) => receive_cw20(deps, env, info, cw20_msg),
        ExecuteMsg::Callback(callback_msg) => handle_callback(deps, env, info, callback_msg),

        ExecuteMsg::UpdateConfig {
            controller,
            boost_fee,
        } => execute_update_config(deps, env, info, controller, boost_fee),
        ExecuteMsg::UpdateParameters {
            max_quota,
            staker_rate,
        } => execute_update_parameters(deps, env, info, max_quota, staker_rate),

        // ExecuteMsg::ControllerVote { votes } => execute_controller_vote(deps, env, info, votes),
        // ExecuteMsg::ExtendLockTime { time } => execute_extend_lock_time(deps, env, info, time),
        // ExecuteMsg::ReconcileGovIncome {} => execute_reconcile_gov_income(deps, env, info),
        // ExecuteMsg::SendIncome {} => execute_send_income(deps, env, info),
        ExecuteMsg::ClaimRewards { lp_tokens } => execute_claim_rewards(deps, env, info, lp_tokens),
        ExecuteMsg::Withdraw { lp_token, amount, } => execute_withdraw(deps, env, info, lp_token, amount),
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
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then a [`ContractError`] is returned,
/// otherwise returns a [`Response`] with the specified attributes if the operation was successful
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let staker_addr = addr_validate_to_lower(deps.api, &cw20_msg.sender)?;
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Deposit {} => execute_deposit(deps, env, info, staker_addr, cw20_msg.amount),
        // Cw20HookMsg::Stake {} => execute_convert(deps, env, info, staker_addr, cw20_msg.amount),
    }
}

/// # Description
/// Handle the callbacks describes in the [`CallbackMsg`]. Returns an [`ContractError`] on failure, otherwise returns the [`Response`]
fn handle_callback(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: CallbackMsg,
) -> Result<Response, ContractError> {
    // Callback functions can only be called by this contract itself
    if info.sender != env.contract.address {
        return Err(ContractError::CallbackUnauthorized {});
    }
    match msg {
        CallbackMsg::AfterClaimed { lp_token } => callback_after_claimed(deps, env, lp_token),
        CallbackMsg::Deposit { lp_token, staker_addr, amount } => callback_deposit(deps, env, lp_token, staker_addr, amount),
        CallbackMsg::Withdraw { lp_token, staker_addr, amount } => callback_withdraw(deps, env, lp_token, staker_addr, amount),
        CallbackMsg::AfterBondChanged { lp_token } => callback_after_bond_changed(deps, env, lp_token),
        CallbackMsg::ClaimRewards { lp_token, staker_addr } => callback_claim_rewards(deps, env, lp_token, staker_addr),
    }
}

/// ## Description
/// Exposes all the queries available in the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let result = match msg {
        QueryMsg::PendingToken { lp_token, user } => to_binary(&query_pending_token(deps, env, lp_token, user)?),
        QueryMsg::Deposit { lp_token, user } => to_binary(&query_deposit(deps, env, lp_token, user)?),
        QueryMsg::Config { } => to_binary(&query_config(deps, env)?),
        QueryMsg::PoolInfo { lp_token } => to_binary(&query_pool_info(deps, env, lp_token)?),
        QueryMsg::UserInfo { lp_token, user } => to_binary(&query_user_info(deps, env, lp_token, user)?),
        QueryMsg::RewardInfo { token } => to_binary(&query_reward_info(deps, env, token)?),
        QueryMsg::State { } => to_binary(&query_state(deps, env)?),
    }?;
    Ok(result)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    Ok(Response::default())
}
