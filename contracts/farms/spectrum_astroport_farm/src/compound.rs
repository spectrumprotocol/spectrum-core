use cosmwasm_std::{
    attr, to_binary, Attribute, Coin, CosmosMsg, Decimal, DepsMut, Env, MessageInfo, QueryRequest,
    Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};

use crate::{
    querier::{query_astroport_pending_token, query_astroport_pool_balance},
    state::{CONFIG, POOL_INFO, STATE},
};

use cw20::Cw20ExecuteMsg;

use astroport::asset::{Asset, AssetInfo};
use astroport::generator::{
    ExecuteMsg as AstroportExecuteMsg,
};
use astroport::pair::{
    Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg, PoolResponse,
    QueryMsg as AstroportPairQueryMsg,
};
use astroport::querier::{query_token_balance, simulate};

use spectrum::astroport_farm::ExecuteMsg;

pub fn compound(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let pair_contract = config.pair_contract;

    if config.controller != deps.api.addr_validate(info.sender.as_str())? {
        return Err(StdError::generic_err("unauthorized"));
    }

    let astro_token = &config.astro_token;

    let pool_info = POOL_INFO.load(deps.storage)?;

    // This get pending (ASTRO) reward
    let pending_token_response = query_astroport_pending_token(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let lp_balance = query_astroport_pool_balance(
        deps.as_ref(),
        &pool_info.staking_token,
        &env.contract.address,
        &config.astroport_generator,
    )?;

    let mut total_astro_swap_amount = Uint128::zero();
    let mut total_astro_commission = Uint128::zero();
    let mut compound_amount = Uint128::zero();

    let mut attributes: Vec<Attribute> = vec![];
    let community_fee = config.community_fee;
    let platform_fee = config.platform_fee;
    let controller_fee = config.controller_fee;
    let total_fee = community_fee + platform_fee + controller_fee;

    // calculate auto-compound and commission in ANC
    let reward = query_token_balance(
        &deps.querier,
        astro_token.clone(),
        env.contract.address.clone(),
    )? + pending_token_response.pending;
    if !reward.is_zero() && !lp_balance.is_zero() {
        let commission = reward * total_fee;
        let astro_amount = reward.checked_sub(commission)?;
        // add commission to total swap amount
        total_astro_commission += commission;
        total_astro_swap_amount += commission;

        compound_amount = astro_amount;

        attributes.push(attr("commission", commission));
        attributes.push(attr("compound_amount", compound_amount));
    }

    POOL_INFO.save(deps.storage, &pool_info)?;

    // get reinvest amount
    let reinvest_amount = compound_amount;
    // split reinvest amount
    let swap_amount = reinvest_amount.multiply_ratio(1u128, 2u128);
    // add commission to reinvest ANC to total swap amount
    total_astro_swap_amount += swap_amount;

    // find ASTRO swap rate
    let astro = Asset {
        info: AssetInfo::Token {
            contract_addr: astro_token.clone(),
        },
        amount: total_astro_swap_amount,
    };
    let astro_swap_rate = simulate(&deps.querier, pair_contract.clone(), &astro)?;
    let total_ust_return_amount = astro_swap_rate.return_amount;
    attributes.push(attr("total_ust_return_amount", total_ust_return_amount));

    let total_ust_commission_amount = if total_astro_swap_amount != Uint128::zero() {
        total_ust_return_amount.multiply_ratio(total_astro_commission, total_astro_swap_amount)
    } else {
        Uint128::zero()
    };
    let total_ust_reinvest_amount =
        total_ust_return_amount.checked_sub(total_ust_commission_amount)?;

    // deduct tax for provided UST
    let net_reinvest_ust = total_ust_reinvest_amount;
    let pool: PoolResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: pair_contract.to_string(),
        msg: to_binary(&AstroportPairQueryMsg::Pool {})?,
    }))?;

    let provide_astro = compute_provide_after_swap(
        &pool,
        &astro,
        astro_swap_rate.return_amount,
        net_reinvest_ust,
    )?;

    let mut messages: Vec<CosmosMsg> = vec![];
    let manual_claim_pending_token = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.astroport_generator.to_string(),
        funds: vec![],
        msg: to_binary(&AstroportExecuteMsg::Withdraw {
            lp_token: pool_info.staking_token.to_string(),
            amount: Uint128::zero(),
        })?,
    });
    messages.push(manual_claim_pending_token);

    if !total_astro_swap_amount.is_zero() {
        let swap_astro: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pair_contract.to_string(),
                amount: total_astro_swap_amount,
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    max_spread: Some(Decimal::percent(50)),
                    belief_price: None,
                    to: None,
                })?,
            })?,
            funds: vec![],
        });
        messages.push(swap_astro);
    }

    if !total_ust_commission_amount.is_zero() {
        // find SPEC swap rate
        let net_commission_amount = total_ust_commission_amount;

        let mut state = STATE.load(deps.storage)?;
        state.earning += net_commission_amount;
        STATE.save(deps.storage, &state)?;

        attributes.push(attr("net_commission", net_commission_amount));

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::SendFee {})?,
            funds: vec![],
        }));
    }

    if !provide_astro.is_zero() {
        let increase_allowance = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: pair_contract.to_string(),
                amount: provide_astro,
                expires: None,
            })?,
            funds: vec![],
        });
        messages.push(increase_allowance);

        let provide_liquidity = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: pair_contract.to_string(),
            msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                assets: [
                    Asset {
                        info: AssetInfo::Token {
                            contract_addr: astro_token.clone(),
                        },
                        amount: provide_astro,
                    },
                    Asset {
                        info: AssetInfo::NativeToken {
                            denom: config.base_denom.clone(),
                        },
                        amount: net_reinvest_ust,
                    },
                ],
                slippage_tolerance: None,
                receiver: None,
                auto_stake: Some(true),
            })?,
            funds: vec![Coin {
                denom: config.base_denom,
                amount: net_reinvest_ust,
            }],
        });
        messages.push(provide_liquidity);
    }

    attributes.push(attr("action", "compound"));
    attributes.push(attr("asset_token", astro_token));
    attributes.push(attr("reinvest_amount", reinvest_amount));
    attributes.push(attr("provide_token_amount", provide_astro));
    attributes.push(attr("provide_ust_amount", net_reinvest_ust));

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

pub fn compute_provide_after_swap(
    pool: &PoolResponse,
    offer: &Asset,
    return_amt: Uint128,
    ask_reinvest_amt: Uint128,
) -> StdResult<Uint128> {
    let (offer_amount, ask_amount) = if pool.assets[0].info == offer.info {
        (pool.assets[0].amount, pool.assets[1].amount)
    } else {
        (pool.assets[1].amount, pool.assets[0].amount)
    };

    let offer_amount = offer_amount + offer.amount;
    let ask_amount = ask_amount.checked_sub(return_amt)?;

    Ok(ask_reinvest_amt.multiply_ratio(offer_amount, ask_amount))
}

pub fn send_fee(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    // only farm contract can execute this message
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }
    let config = CONFIG.load(deps.storage)?;

    let amount = Uint128::zero();
    let aust_token = "";

    let mut messages: Vec<CosmosMsg> = vec![];
    let thousand = Uint128::from(1000u64);
    let total_fee = config.community_fee + config.controller_fee + config.platform_fee;
    let community_amount =
        amount.multiply_ratio(thousand * config.community_fee, thousand * total_fee);
    if !community_amount.is_zero() {
        let transfer_community_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: config.spectrum_gov.to_string(),
                amount: community_amount,
            })?,
            funds: vec![],
        });
        messages.push(transfer_community_fee);
    }

    let platform_amount =
        amount.multiply_ratio(thousand * config.platform_fee, thousand * total_fee);
    if !platform_amount.is_zero() {
        let stake_platform_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: config.platform.to_string(),
                amount: platform_amount,
            })?,
            funds: vec![],
        });
        messages.push(stake_platform_fee);
    }

    let controller_amount = amount.checked_sub(community_amount + platform_amount)?;
    if !controller_amount.is_zero() {
        let stake_controller_fee = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: aust_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: config.controller.to_string(),
                amount: controller_amount,
            })?,
            funds: vec![],
        });
        messages.push(stake_controller_fee);
    }
    Ok(Response::new().add_messages(messages))
}
