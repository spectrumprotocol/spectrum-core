// use std::cmp;
// use cosmwasm_std::{Addr, BankMsg, CosmosMsg, DepsMut, Env, MessageInfo, Order, QuerierWrapper, Response, StdResult, to_binary, Uint128, WasmMsg};
// use cw20::Cw20ExecuteMsg;
// use astroport::asset::{Asset, AssetInfo, token_asset};
// use astroport::querier::query_token_balance;
// use astroport_governance::utils::WEEK;
// use spectrum::adapters::asset::AssetEx;
// use crate::error::ContractError;
// use crate::model::{LockedIncome, RewardInfo};
// use crate::state::{CONFIG, REWARD_INFO, STATE};
//
// pub fn execute_stake(
//     deps: DepsMut,
//     env: Env,
//     info: MessageInfo,
//     staker_addr: Addr,
//     amount: Uint128,
// ) -> Result<Response, ContractError> {
//
//     // deposited token must be xastro
//     let config = CONFIG.load(deps.storage)?;
//     if info.sender != config.astro_gov.xastro_token {
//         return Err(ContractError::Unauthorized {});
//     }
//
//     // check quota
//     let lock = config.astro_gov.query_lock(&deps.querier, env.contract.address)?;
//     if lock.amount + amount > config.max_quota {
//         return Err(ContractError::ExceedQuota(config.max_quota.saturating_sub(lock.amount)));
//     }
//
//     // stake to voting escrow
//     let stake_msg = if lock.amount.is_zero() {
//         config.astro_gov.create_lock_msg(amount, WEEK)?
//     } else {
//         // TODO: check period and extend lock time first
//         config.astro_gov.extend_lock_amount_msg(amount)?
//     };
//
//     // TODO: record user amount
//
//     Ok(Response::new()
//         .add_message(stake_msg)
//         .add_message(mint_msg)
//     )
// }
//
// pub fn execute_controller_vote(
//     deps: DepsMut,
//     _env: Env,
//     info: MessageInfo,
//     votes: Vec<(String, u16)>,
// ) -> Result<Response, ContractError> {
//
//     // only controller can vote
//     let config = CONFIG.load(deps.storage)?;
//     if info.sender != config.controller {
//         return Err(ContractError::Unauthorized {});
//     }
//
//     let vote_msg = config.astro_gov.controller_vote_msg(votes)?;
//
//     Ok(Response::new()
//         .add_message(vote_msg)
//     )
// }
//
// pub fn execute_extend_lock_time(
//     deps: DepsMut,
//     _env: Env,
//     info: MessageInfo,
//     time: u64,
// ) -> Result<Response, ContractError> {
//
//     // only controller can extend lock time
//     let config = CONFIG.load(deps.storage)?;
//     if info.sender != config.controller {
//         return Err(ContractError::Unauthorized {});
//     }
//
//     let extend_lock_time_msg = config.astro_gov.extend_lock_time_msg(time)?;
//
//     Ok(Response::new()
//         .add_message(extend_lock_time_msg)
//     )
// }
//
// // this method is called after claiming income from astro gov, anyone can call this method
// pub fn execute_reconcile_gov_income(
//     deps: DepsMut,
//     env: Env,
//     _info: MessageInfo,
// ) -> Result<Response, ContractError> {
//
//     // load data
//     let config = CONFIG.load(deps.storage)?;
//     let mut state = STATE.load(deps.storage)?;
//     let mut astro_reward = REWARD_INFO.load(deps.storage, &config.astro_token)?;
//
//     // before add claimed amount
//     let now = env.block.time.seconds();
//     astro_reward.realize_unlocked_amount(now);
//
//     // calculate claim
//     let current_period = config.astro_gov.query_last_claim_period(&deps.querier, env.contract.address.clone())?;
//     let target_add_astro_amount = config.astro_gov.calc_claim_amount(
//         &deps.querier,
//         env.contract.address.clone(),
//         state.next_claim_period,
//         current_period,
//     )?;
//     state.next_claim_period = current_period;
//
//     // update amount
//     let astro_amount = query_token_balance(&deps.querier, config.astro_token.clone(), env.contract.address)?;
//     let add_astro_amount = astro_amount.saturating_sub(astro_reward.reconciled_amount);
//     let earned_astro_amount = cmp::min(add_astro_amount, target_add_astro_amount);
//     let fee = earned_astro_amount * config.boost_fee;
//     let net_astro_amount = earned_astro_amount - fee;
//     astro_reward.fee += fee;
//     astro_reward.locked_income = Some(LockedIncome {
//         start: now,
//         end: now + WEEK,
//         amount: net_astro_amount + astro_reward.locked_income.map(|it| it.amount).unwrap_or_default(),
//     });
//     astro_reward.reconciled_amount += earned_astro_amount;
//
//     // save
//     REWARD_INFO.save(deps.storage, &config.astro_token, &astro_reward)?;
//     STATE.save(deps.storage, &state)?;
//
//     Ok(Response::default())
// }
//
// pub fn execute_send_income(
//     deps: DepsMut,
//     env: Env,
//     info: MessageInfo,
// ) -> Result<Response, ContractError> {
//
//     // this method can only invoked by controller
//     let config = CONFIG.load(deps.storage)?;
//     if info.sender != config.controller {
//         return Err(ContractError::Unauthorized {});
//     }
//
//     let mut messages: Vec<CosmosMsg> = vec![];
//     let reward_infos = REWARD_INFO.range(deps.storage, None, None, Order::Ascending)
//         .collect::<StdResult<Vec<(Addr, RewardInfo)>>>()?;
//     for (token, mut reward_info) in reward_infos {
//         reward_info.realize_unlocked_amount(env.block.time.seconds());
//         let staker_income = reward_info.staker_income;
//         let fee = reward_info.fee;
//         reward_info.staker_income = Uint128::zero();
//         reward_info.fee = Uint128::zero();
//         reward_info.reconciled_amount -= staker_income + fee;
//
//         // save
//         REWARD_INFO.save(deps.storage, &token, &reward_info)?;
//
//         // send
//         if !staker_income.is_zero() {
//             let asset = token_asset(token.clone(), staker_income);
//             messages.push(asset.transfer_msg(&config.income_distributor)?);
//         }
//         if !fee.is_zero() {
//             let asset = token_asset(token, fee);
//             messages.push(asset.transfer_msg(&config.fee_distributor)?);
//         }
//     }
//
//     Ok(Response::new()
//         .add_messages(messages)
//     )
// }
