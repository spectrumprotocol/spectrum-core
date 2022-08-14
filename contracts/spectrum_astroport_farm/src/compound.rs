use astroport::{
    asset::{Asset},
};
use cosmwasm_std::{attr, Attribute, Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128};

use crate::{
    error::ContractError,
    state::CONFIG,
};

use cw20::{Expiration};
use astroport::asset::{AssetInfo, AssetInfoExt, token_asset};

use astroport::querier::query_token_balance;
use spectrum::adapters::asset::AssetEx;

use spectrum::astroport_farm::CallbackMsg;

pub fn compound(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    minimum_receive: Option<Uint128>,
) -> Result<Response, ContractError> {

    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.controller {
        return Err(ContractError::Unauthorized {});
    }

    let staking_token = config.liquidity_token;

    let pending_token = config.staking_contract.query_pending_token(
        &deps.querier,
        &staking_token,
        &env.contract.address,
    )?;

    let lp_balance = config.staking_contract.query_deposit(
        &deps.querier,
        &staking_token,
        &env.contract.address,
    )?;

    let thousand = Uint128::from(1000u64);

    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut attributes: Vec<Attribute> = vec![];

    let mut rewards: Vec<Asset> = vec![];
    let mut compound_rewards: Vec<Asset> = vec![];

    let claim_rewards = config.staking_contract.claim_rewards_msg(
        vec![staking_token.to_string()],
    )?;
    messages.push(claim_rewards);

    rewards.push(
        token_asset(config.base_reward_token, pending_token.pending),
    );
    if let Some(pending_on_proxy) = pending_token.pending_on_proxy {
        rewards.extend(pending_on_proxy);
    }

    let mut compound_funds: Vec<Coin> = vec![];
    for asset in rewards {
        let reward_amount = query_token_balance(
            &deps.querier,
            asset.info.to_string(),
            &env.contract.address,
        )? + asset.amount;
        if !reward_amount.is_zero() && !lp_balance.is_zero() {
            let commission_amount = reward_amount * total_fee;
            let compound_amount = reward_amount.checked_sub(commission_amount)?;
            if !compound_amount.is_zero() {
                let compound_asset = asset.info.with_balance(compound_amount);
                if let AssetInfo::NativeToken { denom } = &asset.info {
                    compound_funds.push(Coin { denom: denom.clone(), amount: asset.amount });
                } else {
                    let increase_allowance = compound_asset.increase_allowance_msg(
                        config.compound_proxy.0.to_string(),
                        Some(Expiration::AtHeight(env.block.height + 1)),
                    )?;
                    messages.push(increase_allowance);
                }
                compound_rewards.push(compound_asset);
            }

            let community_amount =
                commission_amount.multiply_ratio(thousand * community_fee, thousand * total_fee);
            if !community_amount.is_zero() {
                let community_asset = asset.info.with_balance(community_amount);
                let transfer_community_fee = community_asset.transfer_msg(&config.community_fee_collector)?;
                messages.push(transfer_community_fee);
            }

            let platform_amount =
                commission_amount.multiply_ratio(thousand * platform_fee, thousand * total_fee);
            if !platform_amount.is_zero() {
                let platform_asset = asset.info.with_balance(platform_amount);
                let transfer_platform_fee = platform_asset.transfer_msg(&config.platform_fee_collector)?;
                messages.push(transfer_platform_fee);
            }

            let controller_amount =
                commission_amount.checked_sub(community_amount + platform_amount)?;
            if !controller_amount.is_zero() {
                let controller_asset = asset.info.with_balance(controller_amount);
                let transfer_controller_fee = controller_asset.transfer_msg(&config.controller_fee_collector)?;
                messages.push(transfer_controller_fee);
            }

            attributes.push(attr("token", asset.info.to_string()));
            attributes.push(attr("compound_amount", compound_amount));
            attributes.push(attr("commission_amount", commission_amount));
        }
    }

    if !compound_rewards.is_empty() {
        let compound = config.compound_proxy.compound_msg(compound_rewards, compound_funds)?;
        messages.push(compound);

        let prev_balance = query_token_balance(&deps.querier, staking_token, &env.contract.address)?;
        messages.push(
            CallbackMsg::Stake {
                prev_balance,
                minimum_receive,
            }
            .into_cosmos_msg(&env.contract.address)?,
        );
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attribute("action", "compound")
        .add_attributes(attributes))
}

pub fn stake(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    prev_balance: Uint128,
    minimum_receive: Option<Uint128>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let staking_token = config.liquidity_token;

    let balance = query_token_balance(&deps.querier, &staking_token, &env.contract.address)?;
    let amount = balance - prev_balance;

    if let Some(minimum_receive) = minimum_receive {
        if amount < minimum_receive {
            return Err(ContractError::AssertionMinimumReceive {
                minimum_receive,
                amount,
            });
        }
    }

    Ok(Response::new()
        .add_message(
            config.staking_contract.deposit_msg(staking_token.to_string(), amount)?
        )
        .add_attributes(vec![
            attr("action", "stake"),
            attr("staking_token", staking_token),
            attr("amount", amount),
        ]))
}
