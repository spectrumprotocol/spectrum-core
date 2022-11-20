#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    from_binary, to_binary, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};

use spectrum::{lp_staking::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    RewardInfoResponse, StateResponse, RewardInfoResponseItem,
}};

use crate::{
    state::{
        read_reward_info, Config, RewardInfo, State, CONFIG, STATE, REWARD_INFOS, query_rewards, OWNERSHIP_PROPOSAL
    },
    ownership::{claim_ownership, drop_ownership_proposal, propose_new_owner}, error::ContractError,
};

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use std::collections::BTreeMap;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            reward_token: deps.api.addr_validate(&msg.reward_token)?,
            staking_token: deps.api.addr_validate(&msg.staking_token)?,
            distribution_schedule: msg.distribution_schedule,
        },
    )?;

    STATE.save(
        deps.storage,
        &State {
            last_distributed: env.block.time.seconds(),
            total_bond_amount: Uint128::zero(),
            global_reward_index: Decimal::zero(),
        },
    )?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Unbond { amount } => unbond(deps, env, info, amount),
        ExecuteMsg::Withdraw {
            amount
        } => withdraw(deps, env, info, amount),
        ExecuteMsg::UpdateConfig {
            distribution_schedule,
        } => update_config(deps, env, info, distribution_schedule),
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

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Bond { staker_addr }) => {
            // only staking token contract can execute this message
            if config.staking_token != info.sender {
                return Err(ContractError::Unauthorized {});
            }

            let cw20_sender = deps.api.addr_validate(&cw20_msg.sender)?.to_string();
            bond(deps, env, staker_addr.unwrap_or(cw20_sender), cw20_msg.amount)
        }
        Err(_) => Err(ContractError::InvalidMessage {}),
    }
}

pub fn bond(deps: DepsMut, env: Env, sender_addr: String, amount: Uint128) -> Result<Response, ContractError> {
    let sender_addr = deps.api.addr_validate(&sender_addr)?;

    let config: Config = CONFIG.load(deps.storage)?;
    let mut state: State = STATE.load(deps.storage)?;
    let mut reward_info: RewardInfo = read_reward_info(deps.storage, &sender_addr)?;

    // Compute global reward & staker reward
    compute_reward(&config, &mut state, env.block.time.seconds());
    compute_staker_reward(&state, &mut reward_info)?;

    // Increase bond_amount
    increase_bond_amount(&mut state, &mut reward_info, amount);

    // Store updated state with staker's reward_info
    REWARD_INFOS.save(deps.storage, &sender_addr, &reward_info)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        ("action", "bond"),
        ("owner", sender_addr.as_str()),
        ("amount", amount.to_string().as_str()),
    ]))
}

pub fn unbond(deps: DepsMut, env: Env, info: MessageInfo, amount: Uint128) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    let sender_addr = info.sender;

    let mut state: State = STATE.load(deps.storage)?;
    let mut reward_info: RewardInfo = read_reward_info(deps.storage, &sender_addr)?;

    if reward_info.bond_amount < amount {
        return Err(ContractError::UnbondExceedBalance {});
    }

    // Compute global reward & staker reward
    compute_reward(&config, &mut state, env.block.time.seconds());
    compute_staker_reward(&state, &mut reward_info)?;

    // Decrease bond_amount
    decrease_bond_amount(&mut state, &mut reward_info, amount)?;

    // Store or remove updated rewards info
    // depends on the left pending reward and bond amount
    if reward_info.pending_reward.is_zero() && reward_info.bond_amount.is_zero() {
        REWARD_INFOS.remove(deps.storage, &sender_addr);
    } else {
        REWARD_INFOS.save(deps.storage, &sender_addr, &reward_info)?;
    }

    // Store updated state
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.staking_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: sender_addr.to_string(),
                amount,
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![
            ("action", "unbond"),
            ("owner", sender_addr.as_str()),
            ("amount", amount.to_string().as_str()),
        ]))
}

// withdraw rewards to executor
pub fn withdraw(deps: DepsMut, env: Env, info: MessageInfo, spec_amount: Option<Uint128>) -> Result<Response, ContractError> {
    let sender_addr = info.sender;

    let config: Config = CONFIG.load(deps.storage)?;
    let mut state: State = STATE.load(deps.storage)?;
    let mut reward_info = read_reward_info(deps.storage, &sender_addr)?;

    // Compute global reward & staker reward
    compute_reward(&config, &mut state, env.block.time.seconds());
    compute_staker_reward(&state, &mut reward_info)?;

    let amount = spec_amount.unwrap_or(reward_info.pending_reward);
    reward_info.pending_reward = reward_info.pending_reward.checked_sub(amount)?;

    // Store or remove updated rewards info
    // depends on the left pending reward and bond amount
    if reward_info.bond_amount.is_zero() && reward_info.pending_reward.is_zero() {
        REWARD_INFOS.remove(deps.storage, &sender_addr);
    } else {
        REWARD_INFOS.save(deps.storage, &sender_addr, &reward_info)?;
    }

    // Store updated state
    STATE.save(deps.storage, &state)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.reward_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: sender_addr.to_string(),
                amount,
            })?,
            funds: vec![],
        })])
        .add_attributes(vec![
            ("action", "withdraw"),
            ("owner", sender_addr.as_str()),
            ("amount", amount.to_string().as_str()),
        ]))
}

pub fn update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    distribution_schedule: Option<Vec<(u64, u64, Uint128)>>,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;
    let state: State = STATE.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(distribution_schedule) = distribution_schedule {
        assert_new_schedules(&config, &state, distribution_schedule.clone())?;
        config.distribution_schedule = distribution_schedule;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![("action", "update_config")]))
}

fn increase_bond_amount(state: &mut State, reward_info: &mut RewardInfo, amount: Uint128) {
    state.total_bond_amount += amount;
    reward_info.bond_amount += amount;
}

fn decrease_bond_amount(
    state: &mut State,
    reward_info: &mut RewardInfo,
    amount: Uint128,
) -> StdResult<()> {
    state.total_bond_amount = state.total_bond_amount.checked_sub(amount)?;
    reward_info.bond_amount = reward_info.bond_amount.checked_sub(amount)?;
    Ok(())
}

// compute distributed rewards and update global reward index
fn compute_reward(config: &Config, state: &mut State, time_seconds: u64) {
    if state.total_bond_amount.is_zero() {
        state.last_distributed = time_seconds;
        return;
    }

    let mut distributed_amount: Uint128 = Uint128::zero();
    for s in config.distribution_schedule.iter() {
        if s.0 > time_seconds || s.1 < state.last_distributed {
            continue;
        }

        let passed_time =
            std::cmp::min(s.1, time_seconds) - std::cmp::max(s.0, state.last_distributed);

        let time = s.1 - s.0;
        let distribution_amount_per_second: Decimal = Decimal::from_ratio(s.2, time);
        distributed_amount += distribution_amount_per_second * Uint128::from(passed_time as u128);
    }

    state.last_distributed = time_seconds;
    state.global_reward_index += Decimal::from_ratio(distributed_amount, state.total_bond_amount);
}

// withdraw reward to pending reward
fn compute_staker_reward(state: &State, reward_info: &mut RewardInfo) -> StdResult<()> {
    let pending_reward = (reward_info.bond_amount * state.global_reward_index)
        .checked_sub(reward_info.bond_amount * reward_info.reward_index)?;

    reward_info.reward_index = state.global_reward_index;
    reward_info.pending_reward += pending_reward;
    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State { time_seconds } => to_binary(&query_state(deps, env, time_seconds)?),
        QueryMsg::RewardInfo { staker_addr, time_seconds} => {
            to_binary(&query_reward_info(deps, env, staker_addr, time_seconds)?)
        },
        QueryMsg::AllRewardInfos { start_after, limit, time_seconds } => {
            to_binary(&query_all_reward_infos(deps, start_after, limit, time_seconds)?)
        },
    }
}

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        owner: config.owner.to_string(),
        reward_token: config.reward_token.to_string(),
        staking_token: config.staking_token.to_string(),
        distribution_schedule: config.distribution_schedule,
    };

    Ok(resp)
}

pub fn query_state(deps: Deps, _env: Env, time_seconds: Option<u64>) -> StdResult<StateResponse> {
    let mut state: State = STATE.load(deps.storage)?;
    if let Some(time_seconds) = time_seconds {
        let config = CONFIG.load(deps.storage)?;
        compute_reward(&config, &mut state, time_seconds);
    }

    Ok(StateResponse {
        last_distributed: state.last_distributed,
        total_bond_amount: state.total_bond_amount,
        global_reward_index: state.global_reward_index,
    })
}

pub fn query_reward_info(
    deps: Deps,
    _env: Env,
    staker_addr: String,
    time_seconds: Option<u64>
) -> StdResult<RewardInfoResponse> {
    let staker_addr = deps.api.addr_validate(&staker_addr)?;

    let mut reward_info: RewardInfo = read_reward_info(deps.storage, &staker_addr)?;

    if let Some(time_seconds) = time_seconds {
        let config = CONFIG.load(deps.storage)?;
        let mut state = STATE.load(deps.storage)?;

        compute_reward(&config, &mut state, time_seconds);
        compute_staker_reward(&state, &mut reward_info)?;
    }

    let config: Config = CONFIG.load(deps.storage)?;

    Ok(RewardInfoResponse {
        staker_addr: staker_addr.to_string(),
        reward_info: RewardInfoResponseItem {
            reward_index: reward_info.reward_index,
            bond_amount: reward_info.bond_amount,
            pending_reward: reward_info.pending_reward,
            staking_token: config.staking_token.to_string(),
        }
    })
}

pub fn assert_new_schedules(
    config: &Config,
    state: &State,
    distribution_schedule: Vec<(u64, u64, Uint128)>,
) -> Result<(), ContractError> {
    if distribution_schedule.len() < config.distribution_schedule.len() {
        return Err(ContractError::InvalidDistributionSchedule {});
    }

    let mut existing_counts: BTreeMap<(u64, u64, Uint128), u32> = BTreeMap::new();
    for schedule in config.distribution_schedule.clone() {
        let counter = existing_counts.entry(schedule).or_insert(0);
        *counter += 1;
    }

    let mut new_counts: BTreeMap<(u64, u64, Uint128), u32> = BTreeMap::new();
    for schedule in distribution_schedule {
        let counter = new_counts.entry(schedule).or_insert(0);
        *counter += 1;
    }

    for (schedule, count) in existing_counts.into_iter() {
        // if began ensure its in the new schedule
        if schedule.0 <= state.last_distributed {
            if count > *new_counts.get(&schedule).unwrap_or(&0u32) {
                return Err(ContractError::DistributionScheduleStarted {});
            }
            // after this new_counts will only contain the newly added schedules
            *new_counts.get_mut(&schedule).unwrap() -= count;
        }
    }

    for (schedule, count) in new_counts.into_iter() {
        if count > 0 && schedule.0 <= state.last_distributed {
            return Err(ContractError::DistributionScheduleStarted {});
        }
    }
    Ok(())
}

pub fn query_all_reward_infos(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
    time_seconds: Option<u64>
) -> StdResult<Vec<RewardInfoResponse>> {
    let reward_infos = query_rewards(
        deps,
        start_after,
        limit)?;
    let mut results: Vec<RewardInfoResponse> = vec![];
    let config: Config = CONFIG.load(deps.storage)?;
    let mut state = STATE.load(deps.storage)?;

    for (addr, mut reward_info) in reward_infos {
        if let Some(time_seconds) = time_seconds {
            compute_reward(&config, &mut state, time_seconds);
            compute_staker_reward(&state, &mut reward_info)?;
        }

        results.push(RewardInfoResponse {
            staker_addr: addr.to_string(),
            reward_info: RewardInfoResponseItem {
                staking_token: config.staking_token.to_string(),
                bond_amount: reward_info.bond_amount,
                reward_index: reward_info.reward_index,
                pending_reward: reward_info.pending_reward
            }
        });
    }

    Ok(results)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
