use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::mock_querier::{mock_dependencies, WasmMockQuerier};
use crate::state::Config;

use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, to_binary, Addr, BankMsg, Coin, CosmosMsg, Decimal, OwnedDeps,
    Response, StdError, Timestamp, Uint128, WasmMsg,
};

use kujira::denom::Denom;
use kujira::msg::{DenomMsg, KujiraMsg};
use kujira::query::KujiraQuery;
use spectrum::adapters::kujira::staking::{ExecuteMsg as StakingExecuteMsg, Staking};
use spectrum::compound_farm::{
    CallbackMsg, ExecuteMsg, InstantiateMsg, QueryMsg, RewardInfoResponse, RewardInfoResponseItem,
};
use spectrum::compound_proxy::{Compounder, ExecuteMsg as CompoundProxyExecuteMsg};
use spectrum::router::Router;

const KUJIRA_TOKEN: &str = "ukuji";
const REWARD_TOKEN: &str = "reward";
const OWNER: &str = "owner";
const USER_1: &str = "user_1";
const USER_2: &str = "user_2";
const USER_3: &str = "user_3";
const BOW_STAKING: &str = "bow_staking";
const ROUTER: &str = "router";
const COMPOUND_PROXY: &str = "compound_proxy";
const CONTROLLER: &str = "controller";
const FEE_COLLECTOR: &str = "fee_collector";
const COMPOUND_PROXY_2: &str = "compound_proxy_2";
const CONTROLLER_2: &str = "controller_2";
const FEE_COLLECTOR_2: &str = "fee_collector_2";
const LP_TOKEN: &str = "factory/market_maker/ulp";
const CLP_TOKEN: &str = "factory/cosmos2contract/market_maker";
const IBC_TOKEN: &str = "ibc/stablecoin";
const MARKET_MAKER: &str = "market_maker";

#[test]
fn test() -> Result<(), ContractError> {
    let mut deps = mock_dependencies();
    create(&mut deps)?;
    config(&mut deps)?;
    owner(&mut deps)?;
    bond(&mut deps)?;
    deposit_time(&mut deps)?;
    compound(&mut deps)?;
    callback(&mut deps)?;

    Ok(())
}

fn assert_error(res: Result<Response<KujiraMsg>, ContractError>, expected: &str) {
    match res {
        Err(ContractError::Std(StdError::GenericErr { msg, .. })) => assert_eq!(expected, msg),
        Err(err) => assert_eq!(expected, format!("{}", err)),
        _ => panic!("Expected exception"),
    }
}

fn create(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, KujiraQuery>,
) -> Result<(), ContractError> {
    let env = mock_env();

    // invalid fee percentage
    let info = mock_info(USER_1, &[]);
    let msg = InstantiateMsg {
        owner: USER_1.to_string(),
        staking: BOW_STAKING.to_string(),
        compound_proxy: COMPOUND_PROXY.to_string(),
        controller: CONTROLLER.to_string(),
        fee: Decimal::percent(101),
        fee_collector: FEE_COLLECTOR.to_string(),
        router: ROUTER.to_string(),
    };
    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_error(res, "fee must be 0 to 1");

    // valid init message
    let msg = InstantiateMsg {
        owner: USER_1.to_string(),
        staking: BOW_STAKING.to_string(),
        compound_proxy: COMPOUND_PROXY.to_string(),
        controller: CONTROLLER.to_string(),
        fee: Decimal::percent(5),
        fee_collector: FEE_COLLECTOR.to_string(),
        router: ROUTER.to_string(),
    };

    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    // query config
    let msg = QueryMsg::Config {};
    let res: Config = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        Config {
            owner: Addr::unchecked(USER_1),
            controller: Addr::unchecked(CONTROLLER),
            fee_collector: Addr::unchecked(FEE_COLLECTOR),
            staking: Staking(Addr::unchecked(BOW_STAKING)),
            compound_proxy: Compounder(Addr::unchecked(COMPOUND_PROXY)),
            fee: Decimal::percent(5),
            router: Router(Addr::unchecked(ROUTER.to_string())),
        }
    );

    Ok(())
}

fn config(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, KujiraQuery>,
) -> Result<(), ContractError> {
    let env = mock_env();

    let info = mock_info(USER_2, &[]);
    let msg = ExecuteMsg::UpdateConfig {
        compound_proxy: None,
        controller: None,
        fee: Some(Decimal::percent(101)),
        fee_collector: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Unauthorized");

    let info = mock_info(USER_1, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_error(res, "fee must be 0 to 1");

    let msg = ExecuteMsg::UpdateConfig {
        compound_proxy: None,
        controller: None,
        fee: Some(Decimal::percent(3)),
        fee_collector: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    let msg = ExecuteMsg::UpdateConfig {
        compound_proxy: Some(COMPOUND_PROXY_2.to_string()),
        controller: None,
        fee: None,
        fee_collector: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    let msg = ExecuteMsg::UpdateConfig {
        compound_proxy: None,
        controller: Some(CONTROLLER_2.to_string()),
        fee: None,
        fee_collector: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    let msg = ExecuteMsg::UpdateConfig {
        compound_proxy: None,
        controller: None,
        fee: None,
        fee_collector: Some(FEE_COLLECTOR_2.to_string()),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    let msg = QueryMsg::Config {};
    let res: Config = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        Config {
            owner: Addr::unchecked(USER_1),
            controller: Addr::unchecked(CONTROLLER_2),
            fee_collector: Addr::unchecked(FEE_COLLECTOR_2),
            staking: Staking(Addr::unchecked(BOW_STAKING)),
            compound_proxy: Compounder(Addr::unchecked(COMPOUND_PROXY_2)),
            fee: Decimal::percent(3),
            router: Router(Addr::unchecked(ROUTER.to_string())),
        }
    );

    let msg = ExecuteMsg::UpdateConfig {
        compound_proxy: Some(COMPOUND_PROXY.to_string()),
        controller: Some(CONTROLLER.to_string()),
        fee: Some(Decimal::percent(5)),
        fee_collector: Some(FEE_COLLECTOR.to_string()),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    let msg = QueryMsg::Config {};
    let res: Config = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        Config {
            owner: Addr::unchecked(USER_1),
            controller: Addr::unchecked(CONTROLLER),
            fee_collector: Addr::unchecked(FEE_COLLECTOR),
            staking: Staking(Addr::unchecked(BOW_STAKING)),
            compound_proxy: Compounder(Addr::unchecked(COMPOUND_PROXY)),
            fee: Decimal::percent(5),
            router: Router(Addr::unchecked(ROUTER.to_string())),
        }
    );

    Ok(())
}

fn owner(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, KujiraQuery>,
) -> Result<(), ContractError> {
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
    let config: Config = from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::Config {})?)?;
    assert_eq!(OWNER, config.owner);
    Ok(())
}

fn bond(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, KujiraQuery>,
) -> Result<(), ContractError> {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(101);

    // mismatch fund
    let info = mock_info(USER_1, &[]);
    let msg = ExecuteMsg::Bond { staker_addr: None };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_error(res, "Invalid funds");

    deps.querier
        .set_supply(CLP_TOKEN.to_string(), Uint128::from(0u128));
    deps.querier.set_supply(
        "factory/market_maker/ulp".to_string(),
        Uint128::from(100000u128),
    );

    // user_1 bond 100000 LP
    let info = mock_info(
        USER_1,
        &[Coin {
            denom: LP_TOKEN.to_string(),
            amount: Uint128::from(100000u128),
        }],
    );
    let msg = ExecuteMsg::Bond { staker_addr: None };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Create {
                subdenom: Denom::from("market_maker")
            })),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BOW_STAKING.to_string(),
                msg: to_binary(&StakingExecuteMsg::Stake { addr: None })?,
                funds: vec![Coin {
                    denom: LP_TOKEN.to_string(),
                    amount: Uint128::from(100000u128),
                }],
            }),
            CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Mint {
                denom: Denom::from(CLP_TOKEN),
                amount: Uint128::from(100000u128),
                recipient: Addr::unchecked("user_1"),
            })),
        ]
    );

    // update staking balance
    deps.querier
        .set_staking_balance(LP_TOKEN.to_string(), Uint128::from(100000u128));
    // update clp supply
    deps.querier
        .set_supply(CLP_TOKEN.to_string(), Uint128::from(100000u128));
    // update user_1 balance
    deps.querier.set_balance(
        CLP_TOKEN.to_string(),
        USER_1.to_string(),
        Uint128::from(100000u128),
    );

    // query reward info
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_1.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_1.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(100000u128),
                    deposit_amount: Uint128::from(100000u128),
                    deposit_time: 101,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(100000u128),
                    deposit_costs: [Uint128::from(10000000u128), Uint128::from(10000000u128)],
                }
            }],
        }
    );

    // update time
    env.block.time = Timestamp::from_seconds(102);

    // user_1 bond 50000 LP for user_2
    let info = mock_info(
        USER_1,
        &[Coin {
            denom: LP_TOKEN.to_string(),
            amount: Uint128::from(50000u128),
        }],
    );
    let msg = ExecuteMsg::Bond {
        staker_addr: Some(USER_2.to_string()),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BOW_STAKING.to_string(),
                msg: to_binary(&StakingExecuteMsg::Stake { addr: None })?,
                funds: vec![Coin {
                    denom: LP_TOKEN.to_string(),
                    amount: Uint128::from(50000u128),
                }],
            }),
            CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Mint {
                denom: Denom::from(CLP_TOKEN),
                amount: Uint128::from(50000u128),
                recipient: Addr::unchecked("user_2"),
            })),
        ]
    );

    // update staking balance
    deps.querier
        .set_staking_balance(LP_TOKEN.to_string(), Uint128::from(150000u128));
    // update clp supply
    deps.querier
        .set_supply(CLP_TOKEN.to_string(), Uint128::from(150000u128));
    // update user_2 balance
    deps.querier.set_balance(
        CLP_TOKEN.to_string(),
        USER_2.to_string(),
        Uint128::from(50000u128),
    );

    // query reward info
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_2.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_2.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(50000u128),
                    deposit_amount: Uint128::from(50000u128),
                    deposit_time: 102,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(50000u128),
                    deposit_costs: [Uint128::from(5000000u128), Uint128::from(5000000u128)],
                }
            }],
        }
    );

    // increase staking balance by 30000 from compound
    deps.querier
        .set_staking_balance(LP_TOKEN.to_string(), Uint128::from(180000u128));
    // update block time by 24 hours
    env.block.time = Timestamp::from_seconds(86502);
    // query reward info for user_1, bond amount should be 100000 + 20000 = 120000
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_1.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_1.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(100000u128),
                    deposit_amount: Uint128::from(100000u128),
                    deposit_time: 101,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(120000u128),
                    deposit_costs: [Uint128::from(10000000u128), Uint128::from(10000000u128)],
                }
            }],
        }
    );

    // query reward info for user_2, bond amount should be 50000 + 10000 = 60000
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_2.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_2.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(50000u128),
                    deposit_amount: Uint128::from(50000u128),
                    deposit_time: 102,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(60000u128),
                    deposit_costs: [Uint128::from(5000000u128), Uint128::from(5000000u128)],
                }
            }],
        }
    );

    // unbond error for new user
    let info = mock_info(
        USER_3,
        &[Coin {
            denom: CLP_TOKEN.to_string(),
            amount: Uint128::from(100u128),
        }],
    );
    let msg = ExecuteMsg::Unbond {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert_error(res, "spectrum_compound_farm::state::RewardInfo not found");

    // unbond for user_1
    let info = mock_info(
        USER_1,
        &[Coin {
            denom: CLP_TOKEN.to_string(),
            amount: Uint128::from(120001u128),
        }],
    );
    let msg = ExecuteMsg::Unbond {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Cannot unbond more than balance");

    let info = mock_info(
        USER_1,
        &[Coin {
            denom: CLP_TOKEN.to_string(),
            amount: Uint128::from(60000u128),
        }],
    );
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Burn {
                denom: Denom::from(CLP_TOKEN),
                amount: Uint128::from(60000u128),
            })),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BOW_STAKING.to_string(),
                msg: to_binary(&StakingExecuteMsg::Withdraw {
                    amount: Coin {
                        denom: LP_TOKEN.to_string(),
                        amount: Uint128::from(72000u128),
                    }
                })?,
                funds: vec![],
            }),
            CosmosMsg::Bank(BankMsg::Send {
                to_address: USER_1.to_string(),
                amount: vec![Coin {
                    denom: LP_TOKEN.to_string(),
                    amount: Uint128::from(72000u128),
                }]
            }),
        ]
    );

    // decrease user_1 balance by 60000 from unbond, 100000 - 60000 = 40000
    deps.querier.set_balance(
        CLP_TOKEN.to_string(),
        USER_1.to_string(),
        Uint128::from(40000u128),
    );
    // decrease clp supply by 60000 from unbond, 150000 - 60000 = 90000
    deps.querier
        .set_supply(CLP_TOKEN.to_string(), Uint128::from(90000u128));
    // decrease staking balance by 72000 from unbond, 180000 - 72000 = 108000
    deps.querier
        .set_staking_balance(LP_TOKEN.to_string(), Uint128::from(108000u128));

    // query reward info for user_1, bond amount should be 120000 - 72000 = 480000
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_1.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_1.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(40000u128),
                    deposit_amount: Uint128::from(40000u128),
                    deposit_time: 101,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(48000u128),
                    deposit_costs: [Uint128::from(4000000u128), Uint128::from(4000000u128)],
                }
            }],
        }
    );

    // query reward info for user_2, should have the same bond amount
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_2.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_2.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(50000u128),
                    deposit_amount: Uint128::from(50000u128),
                    deposit_time: 102,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(60000u128),
                    deposit_costs: [Uint128::from(5000000u128), Uint128::from(5000000u128)],
                }
            }],
        }
    );

    // unbond for user_2, unbond all
    let info = mock_info(
        USER_2,
        &[Coin {
            denom: CLP_TOKEN.to_string(),
            amount: Uint128::from(50000u128),
        }],
    );
    let msg = ExecuteMsg::Unbond {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Burn {
                denom: Denom::from(CLP_TOKEN),
                amount: Uint128::from(50000u128),
            })),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BOW_STAKING.to_string(),
                msg: to_binary(&StakingExecuteMsg::Withdraw {
                    amount: Coin {
                        denom: LP_TOKEN.to_string(),
                        amount: Uint128::from(60000u128),
                    }
                })?,
                funds: vec![],
            }),
            CosmosMsg::Bank(BankMsg::Send {
                to_address: USER_2.to_string(),
                amount: vec![Coin {
                    denom: LP_TOKEN.to_string(),
                    amount: Uint128::from(60000u128),
                }]
            }),
        ]
    );

    // decrease user_2 balance by 50000 from unbond, 50000 - 50000 = 0
    deps.querier.set_balance(
        CLP_TOKEN.to_string(),
        USER_2.to_string(),
        Uint128::from(0u128),
    );
    // decrease clp supply by 50000 from unbond, 90000 - 50000 = 40000
    deps.querier
        .set_supply(CLP_TOKEN.to_string(), Uint128::from(40000u128));
    // decrease staking balance by 60000 from unbond, 108000 - 60000 = 48000
    deps.querier
        .set_staking_balance(LP_TOKEN.to_string(), Uint128::from(48000u128));

    // query reward info for user_2, bond amount should be 60000 - 60000 = 0
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_2.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_2.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(0u128),
                    deposit_amount: Uint128::from(0u128),
                    deposit_time: 102,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(0u128),
                    deposit_costs: [Uint128::from(0u128), Uint128::from(0u128)],
                }
            }],
        }
    );

    // query reward info for user_1, bond amount should be the same
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_1.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_1.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(40000u128),
                    deposit_amount: Uint128::from(40000u128),
                    deposit_time: 101,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(48000u128),
                    deposit_costs: [Uint128::from(4000000u128), Uint128::from(4000000u128)],
                }
            }],
        }
    );

    // update time
    env.block.time = Timestamp::from_seconds(86503);

    // set LP token balance of the contract
    deps.querier.set_balance(
        LP_TOKEN.to_string(),
        MOCK_CONTRACT_ADDR.to_string(),
        Uint128::from(142u128),
    );

    // deposit assets for user_1
    let info = mock_info(
        USER_1,
        &[
            Coin {
                denom: IBC_TOKEN.to_string(),
                amount: Uint128::from(40000u128),
            },
            Coin {
                denom: REWARD_TOKEN.to_string(),
                amount: Uint128::from(20000u128),
            },
        ],
    );
    let msg = ExecuteMsg::BondAssets {
        market_maker: MARKET_MAKER.to_string(),
        minimum_receive: Some(Uint128::from(10000u128)),
        no_swap: None,
        slippage_tolerance: Some(Decimal::percent(2)),
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: COMPOUND_PROXY.to_string(),
                msg: to_binary(&CompoundProxyExecuteMsg::Compound {
                    market_maker: MARKET_MAKER.to_string(),
                    no_swap: None,
                    slippage_tolerance: Some(Decimal::percent(2)),
                })?,
                funds: vec![
                    Coin {
                        denom: IBC_TOKEN.to_string(),
                        amount: Uint128::from(40000u128),
                    },
                    Coin {
                        denom: REWARD_TOKEN.to_string(),
                        amount: Uint128::from(20000u128),
                    },
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::Callback(CallbackMsg::BondTo {
                    to: Addr::unchecked(USER_1),
                    prev_balance: Coin {
                        denom: LP_TOKEN.to_string(),
                        amount: Uint128::from(142u128)
                    },
                    minimum_receive: Some(Uint128::from(10000u128)),
                }))?,
                funds: vec![],
            }),
        ]
    );

    let msg = ExecuteMsg::BondAssets {
        market_maker: MARKET_MAKER.to_string(),
        minimum_receive: Some(Uint128::from(10000u128)),
        no_swap: Some(true),
        slippage_tolerance: None,
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: COMPOUND_PROXY.to_string(),
                msg: to_binary(&CompoundProxyExecuteMsg::Compound {
                    market_maker: MARKET_MAKER.to_string(),
                    no_swap: Some(true),
                    slippage_tolerance: None,
                })?,
                funds: vec![
                    Coin {
                        denom: IBC_TOKEN.to_string(),
                        amount: Uint128::from(40000u128),
                    },
                    Coin {
                        denom: REWARD_TOKEN.to_string(),
                        amount: Uint128::from(20000u128),
                    },
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::Callback(CallbackMsg::BondTo {
                    to: Addr::unchecked(USER_1),
                    prev_balance: Coin {
                        denom: LP_TOKEN.to_string(),
                        amount: Uint128::from(142u128)
                    },
                    minimum_receive: Some(Uint128::from(10000u128)),
                }))?,
                funds: vec![],
            }),
        ]
    );

    // update time
    env.block.time = Timestamp::from_seconds(200201);

    // set LP token balance of the contract
    deps.querier.set_balance(
        LP_TOKEN.to_string(),
        MOCK_CONTRACT_ADDR.to_string(),
        Uint128::from(10141u128),
    );

    let msg = ExecuteMsg::Callback(CallbackMsg::BondTo {
        to: Addr::unchecked(USER_1),
        prev_balance: Coin {
            denom: LP_TOKEN.to_string(),
            amount: Uint128::from(142u128),
        },
        minimum_receive: Some(Uint128::from(10000u128)),
    });
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);
    // received less LP token than minimum_receive, received 10141 - 142 = 9999 LP
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(
        res,
        "Assertion failed; minimum receive amount: 10000, actual amount: 9999",
    );

    deps.querier.set_balance(
        LP_TOKEN.to_string(),
        MOCK_CONTRACT_ADDR.to_string(),
        Uint128::from(10142u128),
    );
    let res = execute(deps.as_mut(), env.clone(), info, msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BOW_STAKING.to_string(),
                msg: to_binary(&StakingExecuteMsg::Stake { addr: None })?,
                funds: vec![Coin {
                    denom: LP_TOKEN.to_string(),
                    amount: Uint128::from(10000u128),
                }],
            }),
            CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Mint {
                denom: Denom::from(CLP_TOKEN),
                amount: Uint128::from(8333u128),
                recipient: Addr::unchecked(USER_1),
            })),
        ]
    );

    deps.querier.set_balance(
        LP_TOKEN.to_string(),
        MOCK_CONTRACT_ADDR.to_string(),
        Uint128::from(142u128),
    );

    // increase user_1 balance by 8333 from unbond, 40000 + 8333 = 48333
    deps.querier.set_balance(
        CLP_TOKEN.to_string(),
        USER_1.to_string(),
        Uint128::from(48333u128),
    );
    // increase clp supply by 48333 from unbond, 40000 + 8333 = 48333
    deps.querier
        .set_supply(CLP_TOKEN.to_string(), Uint128::from(48333u128));
    // increase staking balance by 10000 from bond, 48000 + 10000 = 58000
    deps.querier
        .set_staking_balance(LP_TOKEN.to_string(), Uint128::from(58000u128));

    // query reward info for user_1, bond amount should be 48000 + 10000 = 58000
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_1.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_1.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(48333u128),
                    deposit_amount: Uint128::from(50000u128),
                    deposit_time: 40121,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(58000u128),
                    deposit_costs: [Uint128::from(5000000u128), Uint128::from(5000000u128)],
                }
            }],
        }
    );

    Ok(())
}

fn deposit_time(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, KujiraQuery>,
) -> Result<(), ContractError> {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(300000);

    // user_3 bond 10000 LP
    let info = mock_info(
        USER_3,
        &[Coin {
            denom: LP_TOKEN.to_string(),
            amount: Uint128::from(10000u128),
        }],
    );
    let msg = ExecuteMsg::Bond { staker_addr: None };
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    // update clp supply
    deps.querier
        .set_supply(CLP_TOKEN.to_string(), Uint128::from(56666u128));
    // update user_3 balance
    deps.querier.set_balance(
        CLP_TOKEN.to_string(),
        USER_3.to_string(),
        Uint128::from(8333u128),
    );
    // update staking balance by 10000 + 10000 (from compound), 58000 + 10000 + 10000 = 78000
    deps.querier
        .set_staking_balance(LP_TOKEN.to_string(), Uint128::from(78000u128));

    // query reward info for user_3, should get only 10000
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_3.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_3.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(8333u128),
                    deposit_amount: Uint128::from(10000u128),
                    deposit_time: 300000,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(10000u128),
                    deposit_costs: [Uint128::from(1000000u128), Uint128::from(1000000u128)],
                }
            }],
        }
    );

    env.block.time = Timestamp::from_seconds(343200);

    // query reward info for user_3, should get 10735, 10000 + (10000 * 8333/56666 / 2) = 10735
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_3.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_3.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(8333u128),
                    deposit_amount: Uint128::from(10000u128),
                    deposit_time: 300000,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(10735u128),
                    deposit_costs: [Uint128::from(1000000u128), Uint128::from(1000000u128)],
                }
            }],
        }
    );

    // query reward info for user_1, should be 58000 + (10000 * 48333/56666) = 66529
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_1.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_1.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(48333u128),
                    deposit_amount: Uint128::from(50000u128),
                    deposit_time: 40121,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(66529u128),
                    deposit_costs: [Uint128::from(5000000u128), Uint128::from(5000000u128)],
                }
            }],
        }
    );

    // minimum time reached
    env.block.time = Timestamp::from_seconds(386400);

    // query reward info for user_3, should be 10000 + 1470 = 11470
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_3.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_3.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(8333u128),
                    deposit_amount: Uint128::from(10000u128),
                    deposit_time: 300000,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(11470u128),
                    deposit_costs: [Uint128::from(1000000u128), Uint128::from(1000000u128)],
                }
            }],
        }
    );

    // rewind time
    env.block.time = Timestamp::from_seconds(343200);

    // unbond for user_3
    let info = mock_info(
        USER_3,
        &[Coin {
            denom: CLP_TOKEN.to_string(),
            amount: Uint128::from(8333u128),
        }],
    );
    let msg = ExecuteMsg::Unbond {};
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Burn {
                denom: Denom::from(CLP_TOKEN),
                amount: Uint128::from(8333u128),
            })),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BOW_STAKING.to_string(),
                msg: to_binary(&StakingExecuteMsg::Withdraw {
                    amount: Coin {
                        denom: LP_TOKEN.to_string(),
                        amount: Uint128::from(10735u128),
                    }
                })?,
                funds: vec![],
            }),
            CosmosMsg::Bank(BankMsg::Send {
                to_address: USER_3.to_string(),
                amount: vec![Coin {
                    denom: LP_TOKEN.to_string(),
                    amount: Uint128::from(10735u128),
                }]
            }),
        ]
    );

    // update clp supply
    deps.querier
        .set_supply(CLP_TOKEN.to_string(), Uint128::from(48333u128));
    // update user_3 balance
    deps.querier.set_balance(
        CLP_TOKEN.to_string(),
        USER_3.to_string(),
        Uint128::from(0u128),
    );
    // update staking balance 78000 - 10735 = 67265
    deps.querier
        .set_staking_balance(LP_TOKEN.to_string(), Uint128::from(67265u128));

    // query reward info for user_1, should be 66530 + 735 (from user_3 penalty)= 67265
    let msg = QueryMsg::RewardInfo {
        staker_addr: USER_1.to_string(),
        start_after: None,
        limit: None,
    };
    let res: RewardInfoResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        RewardInfoResponse {
            staker_addr: USER_1.to_string(),
            reward_infos: vec![{
                RewardInfoResponseItem {
                    market_maker: Addr::unchecked(MARKET_MAKER.to_string()),
                    bond_share: Uint128::from(48333u128),
                    deposit_amount: Uint128::from(50000u128),
                    deposit_time: 40121,
                    staking_token: LP_TOKEN.to_string(),
                    bond_amount: Uint128::from(67265u128),
                    deposit_costs: [Uint128::from(5000000u128), Uint128::from(5000000u128)],
                }
            }],
        }
    );

    Ok(())
}

fn compound(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, KujiraQuery>,
) -> Result<(), ContractError> {
    let mut env = mock_env();

    // reset LP token balance of the contract
    deps.querier.set_balance(
        LP_TOKEN.to_string(),
        MOCK_CONTRACT_ADDR.to_string(),
        Uint128::from(1u128),
    );

    // set pending rewards
    deps.querier.set_rewards(vec![
        Coin { denom: IBC_TOKEN.to_string(), amount: Uint128::from(50000u128) },
        Coin { denom: KUJIRA_TOKEN.to_string(), amount: Uint128::from(10000u128) },
    ]);

    // set block height
    env.block.height = 700;

    // only controller can execute compound
    let info = mock_info(USER_1, &[]);
    let msg = ExecuteMsg::Compound {
        market_maker: MARKET_MAKER.to_string(),
        minimum_receive: Some(Uint128::from(29900u128)),
        slippage_tolerance: Some(Decimal::percent(3)),
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(res, "Unauthorized");

    let info = mock_info(CONTROLLER, &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: BOW_STAKING.to_string(),
                msg: to_binary(&StakingExecuteMsg::Claim {
                    denom: Denom::from(LP_TOKEN.to_string()),
                })?,
                funds: vec![],
            }),
            CosmosMsg::Bank(BankMsg::Send {
                to_address: FEE_COLLECTOR.to_string(),
                amount: vec![
                    Coin { denom: IBC_TOKEN.to_string(), amount: Uint128::from(2500u128) },
                    Coin { denom: KUJIRA_TOKEN.to_string(), amount: Uint128::from(500u128) },
                ]
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: COMPOUND_PROXY.to_string(),
                msg: to_binary(&CompoundProxyExecuteMsg::Compound {
                    market_maker: MARKET_MAKER.to_string(),
                    no_swap: None,
                    slippage_tolerance: Some(Decimal::percent(3)),
                })?,
                funds: vec![
                    Coin { denom: IBC_TOKEN.to_string(), amount: Uint128::from(47500u128) },
                    Coin { denom: KUJIRA_TOKEN.to_string(), amount: Uint128::from(9500u128) },
                ],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: MOCK_CONTRACT_ADDR.to_string(),
                msg: to_binary(&ExecuteMsg::Callback(CallbackMsg::Stake {
                    prev_balance: Coin {
                        denom: LP_TOKEN.to_string(),
                        amount: Uint128::from(1u128)
                    },
                    minimum_receive: Some(Uint128::from(29900u128)),
                }))?,
                funds: vec![],
            }),
        ]
    );

    // receive 29899 LP token from compound proxy
    deps.querier.set_balance(
        LP_TOKEN.to_string(),
        MOCK_CONTRACT_ADDR.to_string(),
        Uint128::from(29900u128),
    );

    let msg = ExecuteMsg::Callback(CallbackMsg::Stake {
        prev_balance: Coin {
            denom: LP_TOKEN.to_string(),
            amount: Uint128::from(1u128),
        },
        minimum_receive: Some(Uint128::from(29900u128)),
    });
    let info = mock_info(MOCK_CONTRACT_ADDR, &[]);

    // received less LP token than minimum_receive, received 29900 - 1 = 29899 LP
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert_error(
        res,
        "Assertion failed; minimum receive amount: 29900, actual amount: 29899",
    );

    deps.querier.set_balance(
        LP_TOKEN.to_string(),
        MOCK_CONTRACT_ADDR.to_string(),
        Uint128::from(29901u128),
    );
    let res = execute(deps.as_mut(), env.clone(), info, msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg<KujiraMsg>>>(),
        [            CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: BOW_STAKING.to_string(),
            msg: to_binary(&StakingExecuteMsg::Stake { addr: None })?,
            funds: vec![Coin {
                denom: LP_TOKEN.to_string(),
                amount: Uint128::from(29900u128),
            }],
        }),]
    );

    Ok(())
}

fn callback(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier, KujiraQuery>,
) -> Result<(), ContractError> {
    let env = mock_env();

    let msg = ExecuteMsg::Callback(CallbackMsg::Stake {
        prev_balance: Coin {
            denom: LP_TOKEN.to_string(),
            amount: Uint128::zero(),
        },
        minimum_receive: None,
    });

    let info = mock_info(USER_1, &[]);

    // only contract itself can execute callback
    let res = execute(deps.as_mut(), env.clone(), info, msg);
    assert_error(res, "Unauthorized");

    let msg = ExecuteMsg::Callback(CallbackMsg::BondTo {
        to: Addr::unchecked(USER_1),
        prev_balance: Coin {
            denom: LP_TOKEN.to_string(),
            amount: Uint128::zero(),
        },
        minimum_receive: None,
    });
    let info = mock_info(USER_1, &[]);

    // only contract itself can execute callback
    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_error(res, "Unauthorized");

    Ok(())
}
