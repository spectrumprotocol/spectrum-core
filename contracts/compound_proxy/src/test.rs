use std::str::FromStr;

use cosmwasm_std::testing::{mock_env, mock_info, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    coin, from_binary, to_binary, Addr, Coin, CosmosMsg, Decimal, StdResult, Uint128, WasmMsg,
};

use kujira::fin::ExecuteMsg as FinExecuteMsg;
use spectrum::adapters::kujira::market_maker::{ExecuteMsg as MarketMakerExecuteMsg, MarketMaker};
use spectrum::adapters::pair::Pair;
use spectrum::compound_proxy::{
    CallbackMsg, CompoundSimulationResponse, ExecuteMsg, InstantiateMsg, QueryMsg,
};
use spectrum::router::Router;

use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::mock_querier::mock_dependencies;
use crate::state::Config;

const USER_1: &str = "user_1";
const ROUTER: &str = "router";
const KUJIRA_TOKEN: &str = "ukuji";
const IBC_TOKEN: &str = "ibc/stablecoin";
const PAIR: &str = "fin";
const MARKET_MAKER: &str = "market_maker";
const LP_TOKEN: &str = "factory/market_maker/ulp";

#[test]
fn proper_initialization() -> StdResult<()> {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        router: ROUTER.to_string(),
    };

    let env = mock_env();
    let info = mock_info(USER_1, &[]);
    let res = instantiate(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::Config {};
    let config: Config = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(config.router, Router(Addr::unchecked(ROUTER)),);

    Ok(())
}

#[test]
fn compound() -> Result<(), ContractError> {
    let mut deps = mock_dependencies();
    let env = mock_env();

    let msg = InstantiateMsg {
        router: ROUTER.to_string(),
    };

    let info = mock_info(USER_1, &[]);
    let res = instantiate(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    deps.querier.set_balance(
        KUJIRA_TOKEN.to_string(),
        String::from(MOCK_CONTRACT_ADDR),
        Uint128::new(1000000),
    );

    let info = mock_info(
        USER_1,
        &[Coin {
            denom: KUJIRA_TOKEN.to_string(),
            amount: Uint128::from(1000000u128),
        }],
    );

    let msg = ExecuteMsg::Compound {
        market_maker: MARKET_MAKER.to_string(),
        no_swap: None,
        slippage_tolerance: None,
    };

    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg)?;
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
                    0: CallbackMsg::OptimalSwap {
                        pair: Pair(Addr::unchecked(PAIR.to_string())),
                        market_maker: MarketMaker(Addr::unchecked(MARKET_MAKER)),
                        prev_balances: [
                            Coin {
                                denom: KUJIRA_TOKEN.to_string(),
                                amount: Uint128::zero(),
                            },
                            Coin {
                                denom: IBC_TOKEN.to_string(),
                                amount: Uint128::zero(),
                            },
                        ],
                        slippage_tolerance: None,
                    }
                })?,
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::Callback {
                    0: CallbackMsg::ProvideLiquidity {
                        market_maker: MarketMaker(Addr::unchecked(MARKET_MAKER)),
                        prev_balances: [
                            Coin {
                                denom: KUJIRA_TOKEN.to_string(),
                                amount: Uint128::zero(),
                            },
                            Coin {
                                denom: IBC_TOKEN.to_string(),
                                amount: Uint128::zero(),
                            },
                        ],
                        slippage_tolerance: None,
                    }
                })?,
            }),
        ]
    );

    deps.querier.set_balance(
        KUJIRA_TOKEN.to_string(),
        String::from(MOCK_CONTRACT_ADDR),
        Uint128::new(1000008),
    );
    deps.querier.set_balance(
        IBC_TOKEN.to_string(),
        String::from(MOCK_CONTRACT_ADDR),
        Uint128::new(9),
    );

    let msg = ExecuteMsg::Compound {
        market_maker: MARKET_MAKER.to_string(),
        no_swap: Some(true),
        slippage_tolerance: Some(Decimal::percent(2)),
    };
    let res = execute(deps.as_mut(), env.clone(), info, msg)?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::Callback {
                0: CallbackMsg::ProvideLiquidity {
                    market_maker: MarketMaker(Addr::unchecked(MARKET_MAKER)),
                    prev_balances: [
                        Coin {
                            denom: KUJIRA_TOKEN.to_string(),
                            amount: Uint128::new(8),
                        },
                        Coin {
                            denom: IBC_TOKEN.to_string(),
                            amount: Uint128::new(9),
                        },
                    ],
                    slippage_tolerance: Some(Decimal::percent(2))
                }
            })?,
        }),]
    );

    Ok(())
}

#[test]
fn optimal_swap() -> Result<(), ContractError> {
    let mut deps = mock_dependencies();

    let env = mock_env();

    let msg = InstantiateMsg {
        router: ROUTER.to_string(),
    };

    let info = mock_info(USER_1, &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::Callback {
        0: CallbackMsg::OptimalSwap {
            pair: Pair(Addr::unchecked(PAIR)),
            market_maker: MarketMaker(Addr::unchecked(MARKET_MAKER.to_string())),
            prev_balances: [
                Coin {
                    denom: KUJIRA_TOKEN.to_string(),
                    amount: Uint128::zero(),
                },
                Coin {
                    denom: IBC_TOKEN.to_string(),
                    amount: Uint128::zero(),
                },
            ],
            slippage_tolerance: None,
        },
    };

    let res = execute(deps.as_mut(), env.clone().clone(), info, msg.clone());
    assert_eq!(res, Err(ContractError::Unauthorized {}));

    deps.querier.set_balance(
        KUJIRA_TOKEN.to_string(),
        String::from(MOCK_CONTRACT_ADDR),
        Uint128::from(500000u128),
    );
    deps.querier.set_balance(
        IBC_TOKEN.to_string(),
        String::from(MOCK_CONTRACT_ADDR),
        Uint128::from(1000000u128),
    );

    let info = mock_info(env.contract.address.as_str(), &[]);
    let res = execute(deps.as_mut(), env.clone().clone(), info, msg)?;

    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: PAIR.to_string(),
            funds: vec![Coin {
                denom: IBC_TOKEN.to_string(),
                amount: Uint128::from(250187u128),
            }],
            msg: to_binary(&FinExecuteMsg::Swap {
                offer_asset: Some(Coin {
                    denom: IBC_TOKEN.to_string(),
                    amount: Uint128::from(250187u128),
                }),
                belief_price: None,
                max_spread: None,
                to: None,
            })?,
        }),]
    );

    Ok(())
}

#[test]
fn provide_liquidity() -> Result<(), ContractError> {
    let mut deps = mock_dependencies();

    let env = mock_env();

    deps.querier.set_balance(
        KUJIRA_TOKEN.to_string(),
        String::from(MOCK_CONTRACT_ADDR),
        Uint128::new(1000008),
    );
    deps.querier.set_balance(
        IBC_TOKEN.to_string(),
        String::from(MOCK_CONTRACT_ADDR),
        Uint128::new(1000009),
    );

    let msg = InstantiateMsg {
        router: ROUTER.to_string(),
    };

    let info = mock_info(USER_1, &[]);

    let res = instantiate(deps.as_mut(), env.clone(), info.clone(), msg);
    assert!(res.is_ok());

    let msg = ExecuteMsg::Callback(CallbackMsg::ProvideLiquidity {
        market_maker: MarketMaker(Addr::unchecked(MARKET_MAKER.to_string())),
        prev_balances: [
            Coin {
                denom: IBC_TOKEN.to_string(),
                amount: Uint128::from(9u128),
            },
            Coin {
                denom: KUJIRA_TOKEN.to_string(),
                amount: Uint128::from(8u128),
            },
        ],
        slippage_tolerance: None,
    });

    let res = execute(deps.as_mut(), env.clone(), info, msg.clone());
    assert_eq!(res, Err(ContractError::Unauthorized {}));

    let info = mock_info(env.contract.address.as_str(), &[]);
    let res = execute(deps.as_mut(), env.clone(), info.clone(), msg.clone())?;
    assert_eq!(
        res.messages
            .into_iter()
            .map(|it| it.msg)
            .collect::<Vec<CosmosMsg>>(),
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: MARKET_MAKER.to_string(),
            funds: vec![coin(1000000, IBC_TOKEN), coin(1000000, KUJIRA_TOKEN),],
            msg: to_binary(&MarketMakerExecuteMsg::Deposit {
                max_slippage: None,
                callback: None
            })?,
        }),]
    );

    Ok(())
}

#[test]
fn test_compound_simulation() -> StdResult<()> {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        router: ROUTER.to_string(),
    };

    let env = mock_env();
    let info = mock_info(USER_1, &[]);
    let res = instantiate(deps.as_mut(), env.clone(), info, msg);
    assert!(res.is_ok());

    let msg = QueryMsg::CompoundSimulation {
        market_maker: MARKET_MAKER.to_string(),
        rewards: vec![Coin {
            denom: KUJIRA_TOKEN.to_string(),
            amount: Uint128::from(100u128),
        }],
    };

    deps.querier
        .set_price(KUJIRA_TOKEN.to_string(), Decimal::from_str("2.0")?);
    deps.querier
        .set_supply(LP_TOKEN.to_string(), Uint128::from(1000000u128));

    let res: CompoundSimulationResponse = from_binary(&query(deps.as_ref(), env.clone(), msg)?)?;
    assert_eq!(
        res,
        CompoundSimulationResponse {
            lp_amount: Uint128::from(5u128),
            swap_asset_a_amount: Uint128::from(50u128),
            swap_asset_b_amount: Uint128::from(0u128),
            return_a_amount: Uint128::from(0u128),
            return_b_amount: Uint128::from(100u128),
        }
    );

    Ok(())
}
