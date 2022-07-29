use astroport::{
    asset::{addr_validate_to_lower, Asset, AssetInfo},
    generator::{Cw20HookMsg as AstroportCw20HookMsg, ExecuteMsg as AstroportExecuteMsg},
};
use cosmwasm_std::{
    attr, to_binary, Addr, Attribute, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128,
    WasmMsg,
};

use crate::{
    error::ContractError,
    querier::{query_astroport_pool_balance, query_astroport_reward_info, query_astroport_pending_token},
    state::{CONFIG, POOL_INFO},
};

use cw20::Cw20ExecuteMsg;

use astroport::querier::query_token_balance;

use spectrum::astroport_farm::CallbackMsg;
use spectrum::compound_proxy::ExecuteMsg as CompoundProxyExecuteMsg;

pub fn compound(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    minimum_receive: Option<Uint128>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let pool_info = POOL_INFO.load(deps.storage)?;

    let reward_info = query_astroport_reward_info(
        deps.as_ref(),
        &pool_info.staking_token,
        &config.astroport_generator,
    )?;

    let pending_token_response = query_astroport_pending_token(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator
    )?;
    
    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let thousand = Uint128::from(1000u64);

    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut attributes: Vec<Attribute> = vec![];

    let mut rewards: Vec<(String, Addr, Uint128)> = vec![];
    let mut compound_rewards: Vec<(Addr, Uint128)> = vec![];

    let manual_claim_pending_token = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.astroport_generator.to_string(),
        funds: vec![],
        msg: to_binary(&AstroportExecuteMsg::Withdraw {
            lp_token: pool_info.staking_token.to_string(),
            amount: Uint128::zero(),
        })?,
    });
    messages.push(manual_claim_pending_token);

    rewards.push(("base".to_string(), reward_info.base_reward_token, pending_token_response.pending));
    if let Some(proxy_reward_token) = reward_info.proxy_reward_token {
        let pending_on_proxy = pending_token_response.pending_on_proxy.unwrap_or_else(Uint128::zero);
        rewards.push(("proxy".to_string(), proxy_reward_token, pending_on_proxy));
    }

    for (label, reward_token, pending_amount) in rewards {
        let reward_amount = query_token_balance(
            &deps.querier,
            reward_token.clone(),
            env.contract.address.clone(),
        )? + pending_amount;
        if !reward_amount.is_zero() && !lp_balance.is_zero() {
            let commission_amount = reward_amount * total_fee;
            let compound_amount = reward_amount.checked_sub(commission_amount)?;
            if !compound_amount.is_zero() {
                let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_token.clone().to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                        spender: config.compound_proxy.to_string(),
                        amount: compound_amount,
                        expires: None,
                    })?,
                    funds: vec![],
                });
                messages.push(increase_allowance);
                compound_rewards.push((reward_token.clone(), compound_amount));
            }

            let community_amount =
                commission_amount.multiply_ratio(thousand * community_fee, thousand * total_fee);
            if !community_amount.is_zero() {
                let transfer_community_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: config.community_fee_collector.to_string(),
                        amount: community_amount,
                    })?,
                    funds: vec![],
                });
                messages.push(transfer_community_fee);
            }

            let platform_amount =
                commission_amount.multiply_ratio(thousand * platform_fee, thousand * total_fee);
            if !platform_amount.is_zero() {
                let transfer_platform_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: config.platform_fee_collector.to_string(),
                        amount: platform_amount,
                    })?,
                    funds: vec![],
                });
                messages.push(transfer_platform_fee);
            }

            let controller_amount =
                commission_amount.checked_sub(community_amount + platform_amount)?;
            if !controller_amount.is_zero() {
                let transfer_controller_fee = CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: config.controller_fee_collector.to_string(),
                        amount: controller_amount,
                    })?,
                    funds: vec![],
                });
                messages.push(transfer_controller_fee);
            }

            attributes.push(attr(format!("{}_compound_amount", label), compound_amount));
            attributes.push(attr(
                format!("{}_commission_amount", label),
                commission_amount,
            ));
        }
    }

    if !compound_rewards.is_empty() {
        let rewards = compound_rewards
            .iter()
            .map(|(contract_addr, amount)| Asset {
                info: AssetInfo::Token {
                    contract_addr: addr_validate_to_lower(deps.api, &contract_addr.to_string())
                        .unwrap(),
                },
                amount: *amount,
            })
            .collect();
        let compound = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.compound_proxy.to_string(),
            msg: to_binary(&CompoundProxyExecuteMsg::Compound {
                rewards,
                minimum_receive,
                to: None,
            })?,
            funds: vec![],
        });
        messages.push(compound);
        messages.push(CallbackMsg::Stake {}.into_cosmos_msg(&env.contract.address)?);
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "compound")
        .add_attributes(attributes))
}

pub fn stake(deps: DepsMut, env: Env, _info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let pool_info = POOL_INFO.load(deps.storage)?;

    let astroport_generator = config.astroport_generator;
    let staking_token = pool_info.staking_token;

    let amount = query_token_balance(&deps.querier, staking_token.clone(), env.contract.address)?;

    Ok(Response::new()
        .add_messages(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: staking_token.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: astroport_generator.to_string(),
                amount,
                msg: to_binary(&AstroportCw20HookMsg::Deposit {})?,
            })?,
        })])
        .add_attributes(vec![
            attr("action", "stake"),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}
