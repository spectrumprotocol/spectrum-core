use std::str::FromStr;

use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use crate::state::Config;
use spectrum::lp_staking::ExecuteMsg::UpdateConfig;
use spectrum::lp_staking::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, RewardInfoResponse,
    StateResponse, RewardInfoResponseItem,
};
use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{
    from_binary, to_binary, CosmosMsg, Decimal, StdError, SubMsg, Uint128, WasmMsg, Timestamp, Response,
};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

fn assert_error(res: Result<Response, ContractError>, expected: &str) {
    match res {
        Err(ContractError::Std(StdError::GenericErr {
                                   msg,
                                   ..
                               })) => assert_eq!(expected, msg),
        Err(err) => assert_eq!(expected, format!("{}", err)),
        _ => panic!("Expected exception"),
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        reward_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![(100, 200, Uint128::from(1000000u128))],
    };

    let info = mock_info("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // it worked, let's query the state
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config,
        ConfigResponse {
            owner: "owner0000".to_string(),
            reward_token: "reward0000".to_string(),
            staking_token: "staking0000".to_string(),
            distribution_schedule: vec![(100, 200, Uint128::from(1000000u128))],
        }
    );

    let res = query(
        deps.as_ref(),
        mock_env(),
        QueryMsg::State {
            time_seconds: None
        },
    )
    .unwrap();
    let state: StateResponse = from_binary(&res).unwrap();
    assert_eq!(
        state,
        StateResponse {
            last_distributed: mock_env().block.time.seconds(),
            total_bond_amount: Uint128::zero(),
            global_reward_index: Decimal::zero(),
        }
    );
}

#[test]
fn test_bond_tokens() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        reward_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        owner: "owner0000".to_string(),
        distribution_schedule: vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond { staker_addr: None }).unwrap(),
    });

    let info = mock_info("staking0000", &[]);
    let mut env = mock_env();
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    assert_eq!(
        from_binary::<RewardInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::RewardInfo {
                    staker_addr: "addr0000".to_string(),
                    time_seconds: None
                },
            )
            .unwrap(),
        )
        .unwrap(),
        RewardInfoResponse {
            staker_addr: "addr0000".to_string(),
            reward_info: RewardInfoResponseItem {
                staking_token: "staking0000".to_string(),
                reward_index: Decimal::zero(),
                pending_reward: Uint128::zero(),
                bond_amount: Uint128::from(100u128),
        }
        }
    );

    assert_eq!(
        from_binary::<StateResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::State {
                    time_seconds: None
                }
            )
            .unwrap()
        )
        .unwrap(),
        StateResponse {
            total_bond_amount: Uint128::from(100u128),
            global_reward_index: Decimal::zero(),
            last_distributed: mock_env().block.time.seconds(),
        }
    );

    // bond 100 more tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });
    env.block.time = env.block.time.plus_seconds(10);

    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<RewardInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::RewardInfo {
                    staker_addr: "addr0000".to_string(),
                    time_seconds: None
                },
            )
            .unwrap(),
        )
        .unwrap(),
        RewardInfoResponse {
            staker_addr: "addr0000".to_string(),
            reward_info: RewardInfoResponseItem {
                staking_token: "staking0000".to_string(),
                reward_index: Decimal::from_ratio(1000u128, 1u128),
                pending_reward: Uint128::from(100000u128),
                bond_amount: Uint128::from(200u128),
            }
        }
    );

    assert_eq!(
        from_binary::<StateResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::State {
                    time_seconds: None
                }
            )
            .unwrap()
        )
        .unwrap(),
        StateResponse {
            total_bond_amount: Uint128::from(200u128),
            global_reward_index: Decimal::from_ratio(1000u128, 1u128),
            last_distributed: mock_env().block.time.seconds() + 10,
        }
    );

    // addr0000 bond for addr0001
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond { staker_addr: Some("addr0001".to_string()) }).unwrap(),
    });

    let info = mock_info("staking0000", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    assert_eq!(
        from_binary::<RewardInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::RewardInfo {
                    staker_addr: "addr0001".to_string(),
                    time_seconds: None
                },
            )
            .unwrap(),
        )
        .unwrap(),
        RewardInfoResponse {
            staker_addr: "addr0001".to_string(),
            reward_info: RewardInfoResponseItem {
                staking_token: "staking0000".to_string(),
                reward_index: Decimal::from_ratio(1000u128, 1u128),
                pending_reward: Uint128::zero(),
                bond_amount: Uint128::from(100u128),
        }
        }
    );

    assert_eq!(
        from_binary::<StateResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::State {
                    time_seconds: None
                }
            )
            .unwrap()
        )
        .unwrap(),
        StateResponse {
            total_bond_amount: Uint128::from(300u128),
            global_reward_index: Decimal::from_ratio(1000u128, 1u128),
            last_distributed: mock_env().block.time.seconds() + 10,
        }
    );

    // addr0001 unbond
    let msg = ExecuteMsg::Unbond {
        amount: Uint128::from(100u128),
    };

    let info = mock_info("addr0001", &[]);
    let res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "staking0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0001".to_string(),
                amount: Uint128::from(100u128),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // failed with unautorized
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });

    let info = mock_info("staking0001", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg);
    assert_error(res, "Unauthorized");
}

#[test]
fn test_unbond() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        reward_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![
            (12345, 12345 + 100, Uint128::from(1000000u128)),
            (12345 + 100, 12345 + 200, Uint128::from(10000000u128)),
        ],
    };

    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // bond 100 tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });
    let info = mock_info("staking0000", &[]);
    let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

    // unbond 150 tokens; failed
    let msg = ExecuteMsg::Unbond {
        amount: Uint128::from(150u128),
    };

    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg);
    assert_error(res, "Cannot unbond more than balance");

    // normal unbond
    let msg = ExecuteMsg::Unbond {
        amount: Uint128::from(100u128),
    };

    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "staking0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0000".to_string(),
                amount: Uint128::from(100u128),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );
}

#[test]
fn test_compute_reward() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        reward_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // bond 100 tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });
    let info = mock_info("staking0000", &[]);
    let mut env = mock_env();
    let _res = execute(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

    // 100 seconds passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(100);

    // bond 100 more tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<RewardInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::RewardInfo {
                    staker_addr: "addr0000".to_string(),
                    time_seconds: None
                },
            ).unwrap()
        )
        .unwrap(),
        RewardInfoResponse {
            staker_addr: "addr0000".to_string(),
            reward_info: RewardInfoResponseItem {
                staking_token: "staking0000".to_string(),
                reward_index: Decimal::from_ratio(10000u128, 1u128),
                pending_reward: Uint128::from(1000000u128),
                bond_amount: Uint128::from(200u128),
            }
        }
    );

    // 100 seconds passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(10);
    let info = mock_info("addr0000", &[]);

    // unbond
    let msg = ExecuteMsg::Unbond {
        amount: Uint128::from(100u128),
    };
    let _res = execute(deps.as_mut(), env, info, msg).unwrap();
    assert_eq!(
        from_binary::<RewardInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::RewardInfo {
                    staker_addr: "addr0000".to_string(),
                    time_seconds: None,
                },
            )
            .unwrap()
        )
        .unwrap(),
        RewardInfoResponse {
            staker_addr: "addr0000".to_string(),
            reward_info: RewardInfoResponseItem {
                staking_token: "staking0000".to_string(),
                reward_index: Decimal::from_ratio(15000u64, 1u64),
                pending_reward: Uint128::from(2000000u128),
                bond_amount: Uint128::from(100u128),
            }
        }
    );

    // query future block
    assert_eq!(
        from_binary::<RewardInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::RewardInfo {
                    staker_addr: "addr0000".to_string(),
                    time_seconds: Some(mock_env().block.time.plus_seconds(120).seconds()),
                },
            )
            .unwrap()
        )
        .unwrap(),
        RewardInfoResponse {
            staker_addr: "addr0000".to_string(),
            reward_info: RewardInfoResponseItem {
                staking_token: "staking0000".to_string(),
                reward_index: Decimal::from_ratio(25000u64, 1u64),
                pending_reward: Uint128::from(3000000u128),
                bond_amount: Uint128::from(100u128),
            }
        }
    );
}

#[test]
fn test_withdraw() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        reward_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1_000_000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10_000_000u128),
            ),
        ],
    };

    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    // bond 100 tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });
    let info = mock_info("staking0000", &[]);
    let mut env = mock_env();
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // 100 seconds passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(100);

    let info = mock_info("addr0000", &[]);

    let msg = ExecuteMsg::Withdraw {
        amount: None
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "reward0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0000".to_string(),
                amount: Uint128::from(1_000_000u128),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // test partial withdraw
    // new user bond 100 token
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0001".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });
    let info = mock_info("staking0000", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // 100 seconds passed
    // 10_000_000 rewards distributed to addr0000 and addr0001 5_000_000 each
    env.block.time = env.block.time.plus_seconds(100);

    let info = mock_info("addr0001", &[]);

    // withdraw more than available must error
    let msg = ExecuteMsg::Withdraw {
        amount: Some(Uint128::from(5_000_001u128))
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_err());

    let msg = ExecuteMsg::Withdraw {
        amount: Some(Uint128::from(100u128))
    };
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone());
    assert!(res.is_ok());

    // withdraw another 100
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "reward0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0001".to_string(),
                amount: Uint128::from(100u128),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    // future query of after distribution ended, pending_reward is 5_000_000 - 100 - 100 = 4_999_800
    assert_eq!(
        from_binary::<RewardInfoResponse>(
            &query(
                deps.as_ref(),
                mock_env(),
                QueryMsg::RewardInfo {
                    staker_addr: "addr0001".to_string(),
                    time_seconds: Some(mock_env().block.time.plus_seconds(1000).seconds()),
                },
            )
            .unwrap()
        )
        .unwrap(),
        RewardInfoResponse {
            staker_addr: "addr0001".to_string(),
            reward_info: RewardInfoResponseItem {
                staking_token: "staking0000".to_string(),
                reward_index: Decimal::from_ratio(60000u64, 1u64),
                pending_reward: Uint128::from(4_999_800u128),
                bond_amount: Uint128::from(100u128),
            }
        }
    );

}

#[test]
fn test_update_config() {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        owner: "owner0000".to_string(),
        reward_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(10000000u128),
            ),
        ],
    };

    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let update_config = UpdateConfig {
        distribution_schedule: Some(vec![(
            mock_env().block.time.seconds() + 300,
            mock_env().block.time.seconds() + 400,
            Uint128::from(10000000u128),
        )]),
    };

    let info = mock_info("addr0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config);
    assert_error(res, "Unauthorized");

    // do some bond and update rewards
    // bond 100 tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });
    let info = mock_info("staking0000", &[]);
    let mut env = mock_env();
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // 100 seconds is passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(100);
    let info = mock_info("addr0000", &[]);

    let msg = ExecuteMsg::Withdraw {
        amount: None
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "reward0000".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: "addr0000".to_string(),
                amount: Uint128::from(1000000u128),
            })
            .unwrap(),
            funds: vec![],
        }))]
    );

    let update_config = UpdateConfig {
        distribution_schedule: Some(vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(5000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(10000000u128),
            ),
        ]),
    };

    let info = mock_info("owner0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config);
    assert_error(res, "New distribution schedule already started");

    // do some bond and update rewards
    // bond 100 tokens
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: "addr0000".to_string(),
        amount: Uint128::from(100u128),
        msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
    });
    let info = mock_info("staking0000", &[]);
    let _res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();

    // 100 seconds is passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(100);

    let info = mock_info("addr0000", &[]);

    let msg = ExecuteMsg::Withdraw {
        amount: None
    };
    let _res = execute(deps.as_mut(), env, info, msg).unwrap();

    //cannot update previous scehdule
    let update_config = UpdateConfig {
        distribution_schedule: Some(vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(5000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(10000000u128),
            ),
        ]),
    };

    let info = mock_info("owner0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config);
    assert_error(res, "New distribution schedule already started");

    //successful one
    let update_config = UpdateConfig {
        distribution_schedule: Some(vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(20000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(10000000u128),
            ),
        ]),
    };


    let info = mock_info("owner0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config).unwrap();

    assert_eq!(res.attributes, vec![("action", "update_config")]);

    // query config
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config.distribution_schedule,
        vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(20000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(10000000u128),
            ),
        ]
    );

    //successful one
    let update_config = UpdateConfig {
        distribution_schedule: Some(vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(20000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(50000000u128),
            ),
        ]),
    };


    let info = mock_info("owner0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config).unwrap();

    assert_eq!(res.attributes, vec![("action", "update_config")]);

    // query config
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config.distribution_schedule,
        vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(20000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(50000000u128),
            ),
        ]
    );

    let update_config = UpdateConfig {
        distribution_schedule: Some(vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(90000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(80000000u128),
            ),
        ]),
    };

    let info = mock_info("owner0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config).unwrap();

    assert_eq!(res.attributes, vec![("action", "update_config")]);

    // query config
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config.distribution_schedule,
        vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(90000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(80000000u128),
            ),
        ]
    );

    let update_config = UpdateConfig {
        distribution_schedule: Some(vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(90000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(80000000u128),
            ),
            (
                mock_env().block.time.seconds() + 500,
                mock_env().block.time.seconds() + 600,
                Uint128::from(60000000u128),
            ),
        ]),
    };

    let info = mock_info("owner0000", &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config).unwrap();

    assert_eq!(res.attributes, vec![("action", "update_config")]);

    // query config
    let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
    let config: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config.distribution_schedule,
        vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1000000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 200,
                mock_env().block.time.seconds() + 300,
                Uint128::from(10000000u128),
            ),
            (
                mock_env().block.time.seconds() + 300,
                mock_env().block.time.seconds() + 400,
                Uint128::from(90000000u128),
            ),
            (
                mock_env().block.time.seconds() + 400,
                mock_env().block.time.seconds() + 500,
                Uint128::from(80000000u128),
            ),
            (
                mock_env().block.time.seconds() + 500,
                mock_env().block.time.seconds() + 600,
                Uint128::from(60000000u128),
            )
        ]
    );
}

#[test]
fn test_query_all_rewards() {
    let mut deps = mock_dependencies(&[]);
    let addr0 = "addr0";
    let addr1 = "addr1";
    let addr2 = "addr2";
    let addr3 = "addr3";
    let addr4 = "addr4";
    let addr5 = "addr5";
    let addr6 = "addr6";
    let addr7 = "addr7";
    let all_addresses = vec![addr0, addr1, addr2, addr3, addr4, addr5, addr6, addr7];

    let reward0000 = "reward0000";
    let owner0000 = "owner0000";
    let staking0000 = "staking0000";


    let msg = InstantiateMsg {
        owner: owner0000.to_string(),
        reward_token: reward0000.to_string(),
        staking_token: staking0000.to_string(),
        distribution_schedule: vec![
            (
                mock_env().block.time.seconds(),
                mock_env().block.time.seconds() + 100,
                Uint128::from(1_000_000u128),
            ),
            (
                mock_env().block.time.seconds() + 100,
                mock_env().block.time.seconds() + 200,
                Uint128::from(10_000_000u128),
            ),
        ],
    };

    let info = mock_info(owner0000, &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let mut env = mock_env();
    for addr in all_addresses.iter(){
        // bond 100 tokens
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: addr.to_string(),
            amount: Uint128::from(100u128),
            msg: to_binary(&Cw20HookMsg::Bond {staker_addr: None}).unwrap(),
        });
        let info = mock_info(staking0000, &[]);
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    }

    // 100 seconds passed
    // 1,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(100);

    // addr4 withdraw reward
    let info = mock_info(addr4, &[]);
    let msg = ExecuteMsg::Withdraw {
        amount: Some(Uint128::from(100u128))
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward0000.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: addr4.to_string(),
                amount: Uint128::from(100u128),
            }).unwrap(),
            funds: vec![],
        }))]
    );

    // TODO start_after is bugged, it does not affect anything.
    let res: Vec<RewardInfoResponse> = from_binary(&query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::AllRewardInfos {
            start_after: None,
            limit: Some(5),
            time_seconds: None
        },
    ).unwrap()).unwrap();

    assert_eq!(
        res,
        vec![
            RewardInfoResponse {
                staker_addr: addr0.to_string(),
                reward_info: RewardInfoResponseItem {
                    staking_token: staking0000.to_string(),
                    reward_index: Decimal::from_str("0").unwrap(),
                    pending_reward: Uint128::from(0u128),
                    bond_amount: Uint128::from(100u128),
                }
            },
            RewardInfoResponse {
                staker_addr: addr1.to_string(),
                reward_info: RewardInfoResponseItem {
                    staking_token: staking0000.to_string(),
                    reward_index: Decimal::from_str("0").unwrap(),
                    pending_reward: Uint128::from(0u128),
                    bond_amount: Uint128::from(100u128),
                }
            },
            RewardInfoResponse {
                staker_addr: addr2.to_string(),
                reward_info: RewardInfoResponseItem {
                    staking_token: staking0000.to_string(),
                    reward_index: Decimal::from_str("0").unwrap(),
                    pending_reward: Uint128::from(0u128),
                    bond_amount: Uint128::from(100u128),
                }
            },
            RewardInfoResponse {
                staker_addr: addr3.to_string(),
                reward_info: RewardInfoResponseItem {
                    staking_token: staking0000.to_string(),
                    reward_index: Decimal::from_str("0").unwrap(),
                    pending_reward: Uint128::from(0u128),
                    bond_amount: Uint128::from(100u128),
                }
            },
            RewardInfoResponse {
                staker_addr: addr4.to_string(),
                reward_info: RewardInfoResponseItem {
                    staking_token: staking0000.to_string(),
                    reward_index: Decimal::from_str("1250").unwrap(),
                    pending_reward: Uint128::from(124900u128),
                    bond_amount: Uint128::from(100u128),
                }
            },
        ]
    );

    // 100 seconds passed
    // 10,000,000 rewards distributed
    env.block.time = env.block.time.plus_seconds(100);

    // addr4 withdraw reward
    let info = mock_info(addr4, &[]);
    let msg = ExecuteMsg::Withdraw {
        amount: Some(Uint128::from(100u128))
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward0000.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: addr4.to_string(),
                amount: Uint128::from(100u128),
            }).unwrap(),
            funds: vec![],
        }))]
    );

    // input time second to calculate reward for staker that has never withdraw
    let res: Vec<RewardInfoResponse> = from_binary(&query(
        deps.as_ref(),
        env.clone(),
        QueryMsg::AllRewardInfos {
            start_after: Some(addr3.to_string()),
            limit: Some(2),
            time_seconds: Some(env.clone().block.time.seconds())
        },
    ).unwrap()).unwrap();

    assert_eq!(
        res,
        vec![
            RewardInfoResponse {
                staker_addr: addr4.to_string(),
                reward_info: RewardInfoResponseItem {
                    staking_token: staking0000.to_string(),
                    reward_index: Decimal::from_str("13750").unwrap(),
                    pending_reward: Uint128::from(1374800u128),
                    bond_amount: Uint128::from(100u128),
                }
            },
            RewardInfoResponse {
                staker_addr: addr5.to_string(),
                reward_info: RewardInfoResponseItem {
                    staking_token: staking0000.to_string(),
                    reward_index: Decimal::from_str("13750").unwrap(),
                    pending_reward: Uint128::from(1375000u128),
                    bond_amount: Uint128::from(100u128),
                }
            },
        ]
    );
}

#[test]
fn owner() {
    let mut env = mock_env();
    let mut deps = mock_dependencies(&[]);
    const OWNER: &str = "owner";
    const USER_1: &str = "user_1";
    const USER_2: &str = "user_2";
    const USER_3: &str = "user_3";

    let msg = InstantiateMsg {
        owner: USER_1.to_string(),
        reward_token: "reward0000".to_string(),
        staking_token: "staking0000".to_string(),
        distribution_schedule: vec![(100, 200, Uint128::from(1000000u128))],
    };

    let info = mock_info("addr0000", &[]);
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
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
    )
    .unwrap();
    assert_eq!(0, res.messages.len());

    // query config
    let config: Config =
        from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(OWNER, config.owner);
}