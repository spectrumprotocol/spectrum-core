use std::str::FromStr;

use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage};
use cosmwasm_std::{
    to_binary, Addr, Coin, CosmosMsg, OwnedDeps, Response, StdError, Uint128, WasmMsg, Decimal256, Timestamp, from_binary,
};

use kujira::denom::Denom;
use spectrum::router::{ExecuteMsg, InstantiateMsg, QueryMsg, SwapOperationRequest};
use kujira::fin::ExecuteMsg as FinExecuteMsg;
use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::{Config, CONFIG};

const OWNER: &str = "owner";
const USER_1: &str = "user_1";
const USER_2: &str = "user_2";
const USER_3: &str = "user_2";
const TOKEN_1: &str = "token_1";
const TOKEN_2: &str = "token_2";
const IBC_TOKEN: &str = "ibc/stablecoin";

#[test]
fn test() -> Result<(), ContractError> {
    let mut deps = mock_dependencies();
    create(&mut deps)?;
    owner(&mut deps)?;
    config(&mut deps)?;
    swap(&mut deps)?;
    // callback(&mut deps)?;

    Ok(())
}

fn assert_error(res: Result<Response, ContractError>, expected: &str) {
    match res {
        Err(ContractError::Std(StdError::GenericErr { msg, .. })) => assert_eq!(expected, msg),
        Err(err) => assert_eq!(expected, format!("{}", err)),
        _ => panic!("Expected exception"),
    }
}

fn create(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
) -> Result<(), ContractError> {
    let env = mock_env();
    let info = mock_info(USER_1, &[]);

    let instantiate_msg = InstantiateMsg {
        owner: USER_1.to_string(),
    };
    let res = instantiate(deps.as_mut(), env, info, instantiate_msg);
    assert!(res.is_ok());

    let config = CONFIG.load(deps.as_mut().storage)?;
    assert_eq!(
        config,
        Config {
            owner: Addr::unchecked(USER_1.to_string()),
        }
    );

    Ok(())
}

fn owner(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> Result<(), ContractError> {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(0);

    // new owner
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: OWNER.to_string(),
        expires_in: 100,
    };

    let info = mock_info(USER_2, &[]);

    // unauthorized check
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_error(res, "Unauthorized");

    // claim before a proposal
    let info = mock_info(USER_2, &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    );
    assert_error(res, "Ownership proposal not found");

    // propose new owner
    let info = mock_info(USER_1, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert!(res.is_ok());

    // drop ownership proposal
    let info = mock_info(USER_1, &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::DropOwnershipProposal {},
    );
    assert!(res.is_ok());

    // ownership proposal dropped
    let info = mock_info(USER_2, &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    );
    assert_error(res, "Ownership proposal not found");

    // propose new owner again
    let info = mock_info(USER_1, &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // unauthorized ownership claim
    let info = mock_info(USER_3, &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    );
    assert_error(res, "Unauthorized");

    env.block.time = Timestamp::from_seconds(101);

    // ownership proposal expired
    let info = mock_info(OWNER, &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    );
    assert_error(res, "Ownership proposal expired");

    env.block.time = Timestamp::from_seconds(100);

    // claim ownership
    let info = mock_info(OWNER, &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )?;
    assert_eq!(0, res.messages.len());

    // query config
    let config: Config =
        from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::Config {})?)?;
    assert_eq!(OWNER, config.owner);
    Ok(())
}

fn create_swap_op(ask: String, offer: String) -> SwapOperationRequest {
    SwapOperationRequest { pair: get_key(&offer, &ask), offer: Denom::from(offer), ask: Denom::from(ask) }
}

fn get_key(
    offer: &String,
    ask: &String,
) -> String {
    if offer > ask {
        format!("{0}|{1}", ask, offer)
    } else {
        format!("{0}|{1}", offer, ask)
    }
}

fn config(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> Result<(), ContractError> {
    let env = mock_env();

    let info = mock_info(USER_1, &[]);
    let msg = ExecuteMsg::UpsertRoute { operations: vec![
        create_swap_op(TOKEN_2.to_string(), TOKEN_1.to_string()),    
    ] };
    // unauthorized check
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_error(res, "Unauthorized");

    let info = mock_info(OWNER, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::UpsertRoute { operations: vec![
        create_swap_op(TOKEN_2.to_string(), IBC_TOKEN.to_string()),    
    ] };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    Ok(())
}


fn swap(deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>) -> Result<(), ContractError> {
    let env = mock_env();

    let info = mock_info(USER_1, &[
        Coin {
            denom: TOKEN_1.to_string(),
            amount: Uint128::from(100u128),
        }
    ]);
    let msg = ExecuteMsg::Swap {
        belief_price: Some(Decimal256::percent(100)),
        max_spread: Some(Decimal256::percent(1)),
        to: Some(USER_2.to_string()),
        ask: Denom::from(TOKEN_2.to_string()),
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        [CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: format!("{0}|{1}", TOKEN_1, TOKEN_2),
            msg: to_binary(&FinExecuteMsg::Swap {
                offer_asset: Some(Coin { denom: TOKEN_1.to_string(), amount: Uint128::from(100u128) }),
                belief_price: Some(Decimal256::from_str("1")?),
                max_spread: Some(Decimal256::from_str("0.01")?),
                to: Some(Addr::unchecked(USER_2.to_string())),
            })?,
            funds: vec![Coin {
                denom: TOKEN_1.to_string(),
                amount: Uint128::from(100u128),
            }],
        }),]
    );

    let info = mock_info(
        USER_1,
        &[Coin {
            denom: TOKEN_2.to_string(),
            amount: Uint128::from(100u128),
        }],
    );
    let msg = ExecuteMsg::Swap {
        belief_price: Some(Decimal256::percent(100)),
        max_spread: Some(Decimal256::percent(1)),
        to: None,
        ask: Denom::from(IBC_TOKEN.to_string()),
    };

    let res = execute(deps.as_mut(), env, info, msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        [CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: format!("{0}|{1}", IBC_TOKEN, TOKEN_2),
            msg: to_binary(&FinExecuteMsg::Swap {
                offer_asset: Some(Coin { denom: TOKEN_2.to_string(), amount: Uint128::from(100u128) }),
                belief_price: Some(Decimal256::from_str("1")?),
                max_spread: Some(Decimal256::from_str("0.01")?),
                to: Some(Addr::unchecked(USER_1.to_string())),
            })?,
            funds: vec![Coin {
                denom: TOKEN_2.to_string(),
                amount: Uint128::from(100u128),
            }],
        }),]
    );

    Ok(())
}
