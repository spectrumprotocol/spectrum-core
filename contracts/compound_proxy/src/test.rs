use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::{
    Cw20HookMsg as AstroportPairCw20HookMsg, ExecuteMsg as AstroportPairExecuteMsg,
};
use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{coin, to_binary, Addr, Coin, CosmosMsg, Decimal, Order, StdResult, Uint128, WasmMsg, from_binary, Uint256};
use cw20::{Cw20ExecuteMsg, Expiration};
use spectrum::adapters::pair::Pair;
use spectrum::compound_proxy::{CallbackMsg, ExecuteMsg, InstantiateMsg, QueryMsg};

use crate::contract::{execute, get_swap_amount, instantiate, query};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use crate::state::{Config, PAIR_PROXY};

#[test]
fn proper_initialization() -> StdResult<()> {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pair_contract: "pair_contract".to_string(),
        commission_bps: 30,
        pair_proxies: vec![
            (
                AssetInfo::Token {
                    contract_addr: Addr::unchecked("token0001"),
                },
                "pair0001".to_string(),
            ),
            (
                AssetInfo::NativeToken {
                    denom: "ibc/token".to_string(),
                },
                "pair0002".to_string(),
            ),
        ],
        slippage_tolerance: Decimal::percent(1),
    };

    let sender = "addr0000";

    let env = mock_env();
    let info = mock_info(sender, &[]);
    let res = instantiate(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::Config {};
    let config: Config = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        config.pair_info,
        PairInfo {
            asset_infos: vec![
                {
                    AssetInfo::Token {
                        contract_addr: Addr::unchecked("token"),
                    }
                },
                {
                    AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    }
                }
            ],
            contract_addr: Addr::unchecked("pair_contract"),
            liquidity_token: Addr::unchecked("liquidity_token"),
            pair_type: astroport::factory::PairType::Xyk {}
        }
    );

    let pair_proxies = PAIR_PROXY
        .range(&deps.storage, None, None, Order::Ascending)
        .collect::<StdResult<Vec<(String, Pair)>>>()?;
    assert_eq!(
        pair_proxies,
        vec![
            ("ibc/token".to_string(), Pair(Addr::unchecked("pair0002"))),
            ("token0001".to_string(), Pair(Addr::unchecked("pair0001"))),
        ]
    );

    Ok(())
}

#[test]
fn compound() -> Result<(), ContractError> {
    let mut deps = mock_dependencies(&[]);

    let msg = InstantiateMsg {
        pair_contract: "pair_contract".to_string(),
        commission_bps: 30,
        pair_proxies: vec![],
        slippage_tolerance: Decimal::percent(1),
    };

    let sender = "addr0000";

    let env = mock_env();
    let info = mock_info(sender, &[]);
    let res = instantiate(deps.as_mut(), env, info, msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::Compound {
        rewards: vec![Asset {
            info: AssetInfo::NativeToken {
                denom: "uluna".to_string(),
            },
            amount: Uint128::from(1000000u128),
        }],
        to: None,
    };

    let env = mock_env();
    let info = mock_info(
        "addr0000",
        &[Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(1000000u128),
        }],
    );

    let res = execute(deps.as_mut(), env.clone(), info, msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::Callback {
                    0: CallbackMsg::OptimalSwap {}
                })?,
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::Callback {
                    0: CallbackMsg::ProvideLiquidity {
                        receiver: "addr0000".to_string()
                    }
                })?,
            }),
        ]
    );

    Ok(())
}

#[test]
fn optimal_swap() -> Result<(), ContractError> {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance(&[(
        &String::from("pair_contract"),
        &[Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(1000000000),
        }],
    )]);
    deps.querier.with_token_balances(&[(
        &String::from("token"),
        &[
            (&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(1000000)),
            (&String::from("pair_contract"), &Uint128::new(1000000000)),
        ],
    )]);

    let env = mock_env();

    let msg = InstantiateMsg {
        pair_contract: "pair_contract".to_string(),
        commission_bps: 30,
        pair_proxies: vec![],
        slippage_tolerance: Decimal::percent(1),
    };

    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::Callback {
        0: CallbackMsg::OptimalSwap {},
    };

    let res = execute(deps.as_mut(), env.clone().clone(), info, msg.clone());
    assert_eq!(res, Err(ContractError::Unauthorized {}));

    let info = mock_info(env.contract.address.as_str(), &[]);
    let res = execute(deps.as_mut(), env.clone().clone(), info, msg)?;

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "token".to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: "pair_contract".to_string(),
                amount: Uint128::new(500626),
                msg: to_binary(&AstroportPairCw20HookMsg::Swap {
                    ask_asset_info: None,
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })?
            })?,
        }),]
    );

    Ok(())
}

#[test]
fn provide_liquidity() -> Result<(), ContractError> {
    let mut deps = mock_dependencies(&[]);
    deps.querier.with_balance(&[
        (
            &String::from("pair_contract"),
            &[Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(1000000000),
            }],
        ),
        (
            &String::from(MOCK_CONTRACT_ADDR),
            &[Coin {
                denom: "uluna".to_string(),
                amount: Uint128::new(1000000),
            }],
        ),
    ]);
    deps.querier.with_token_balances(&[(
        &String::from("token"),
        &[
            (&String::from(MOCK_CONTRACT_ADDR), &Uint128::new(1000000)),
            (&String::from("pair_contract"), &Uint128::new(1000000000)),
        ],
    )]);

    let env = mock_env();

    let msg = InstantiateMsg {
        pair_contract: "pair_contract".to_string(),
        commission_bps: 30,
        pair_proxies: vec![],
        slippage_tolerance: Decimal::percent(1),
    };

    let info = mock_info("addr0000", &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::Callback {
        0: CallbackMsg::ProvideLiquidity {
            receiver: "sender".to_string(),
        },
    };

    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_eq!(res, Err(ContractError::Unauthorized {}));

    let info = mock_info(env.contract.address.as_str(), &[]);
    let res = execute(deps.as_mut(), env, info, msg)?;

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "token".to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                    spender: "pair_contract".to_string(),
                    amount: Uint128::from(1000000u128),
                    expires: Some(Expiration::AtHeight(12346)),
                })?,
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: "pair_contract".to_string(),
                funds: vec![coin(1000000, "uluna")],
                msg: to_binary(&AstroportPairExecuteMsg::ProvideLiquidity {
                    assets: vec![
                        Asset {
                            info: AssetInfo::Token {
                                contract_addr: Addr::unchecked("token"),
                            },
                            amount: Uint128::from(1000000u128),
                        },
                        Asset {
                            info: AssetInfo::NativeToken {
                                denom: "uluna".to_string(),
                            },
                            amount: Uint128::from(1000000u128),
                        },
                    ],
                    slippage_tolerance: Some(Decimal::percent(1)),
                    auto_stake: None,
                    receiver: Some("sender".to_string()),
                })?,
            }),
        ]
    );

    Ok(())
}

#[test]
fn test_get_swap_amount() -> StdResult<()> {
    let amount_a = Uint256::from(1146135045u128);
    let amount_b = Uint256::from(9093887u128);
    let pool_a = Uint256::from(114613504500u128);
    let pool_b = Uint256::from(909388700u128);
    let commission_bps = 30u64;

    let result = get_swap_amount(
        amount_a,
        amount_b,
        pool_a,
        pool_b,
        commission_bps,
    )?;

    assert_eq!(result, Uint128::zero());

    Ok(())
}
