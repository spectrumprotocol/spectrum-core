use cosmwasm_std::testing::{MockApi, MockStorage};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, BalanceResponse, BankQuery, Binary, Coin,
    ContractResult, Decimal, OwnedDeps, Querier, QuerierResult, QueryRequest, StdResult,
    SystemError, SystemResult, Uint128, WasmQuery, Empty, Uint256, StdError,
};
use kujira::asset::{Asset, AssetInfo};
use kujira::fin::SimulationResponse;
use spectrum::router::Route;
use std::collections::HashMap;

use kujira::denom::Denom;
use kujira::precision::Precision;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum::adapters::kujira::market_maker::{ConfigResponse, PoolResponse};

pub fn mock_dependencies() -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new();

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
        custom_query_type: Default::default(),
    }
}

pub struct WasmMockQuerier {
    balances: HashMap<(String, String), Uint128>,
    raw: HashMap<(String, Binary), Binary>,
    prices: HashMap<(String, String), Decimal>, // price = ask / offer
}

impl WasmMockQuerier {
    pub fn new() -> Self {
        WasmMockQuerier {
            balances: HashMap::new(),
            raw: HashMap::new(),
            prices: HashMap::new(),
        }
    }

    pub fn set_balance(&mut self, token: String, addr: String, amount: Uint128) {
        self.balances.insert((token, addr), amount);
    }

    fn get_balance(&self, token: String, addr: String) -> Uint128 {
        *self
            .balances
            .get(&(token, addr))
            .unwrap_or(&Uint128::zero())
    }

    pub fn set_price(&mut self, ask: String, offer: String, price: Decimal) {
        self.prices.insert((ask, offer), price);
    }

    fn get_price(&self, ask: String, offer: String) -> Option<&Decimal> {
        self.prices.get(&(ask, offer))
    }

    fn execute_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        let result = match request {
            QueryRequest::Bank(BankQuery::Balance { address, denom }) => {
                let amount = self.get_balance(denom.clone(), address.clone());
                to_binary(&BalanceResponse {
                    amount: Coin {
                        denom: denom.clone(),
                        amount,
                    },
                })
            }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                self.execute_wasm_query(contract_addr, msg)
            }
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                let value = self.raw.get(&(contract_addr.clone(), key.clone()));
                if let Some(binary) = value {
                    Ok(binary.clone())
                } else {
                    Ok(Binary::default())
                }
            }
            _ => return QuerierResult::Err(SystemError::Unknown {}),
        };
        QuerierResult::Ok(ContractResult::from(result))
    }

    fn execute_wasm_query(&self, _: &String, msg: &Binary) -> StdResult<Binary> {
        match from_binary(msg)? {
            MockQueryMsg::Pool {} => to_binary(&PoolResponse {
                balances: [Uint128::from(10000000u128), Uint128::from(10000000u128)],
            }),
            MockQueryMsg::Config { .. } => to_binary(&ConfigResponse {
                owner: Addr::unchecked("owner"),
                denoms: [Denom::from("ukuji"), Denom::from("ibc/stablecoin")],
                price_precision: Precision::DecimalPlaces(4u8),
                decimal_delta: 0,
                fin_contract: Addr::unchecked("fin"),
                intervals: vec![Decimal::percent(1)],
                fee: Decimal::from_ratio(3u128, 1000u128),
                amp: Decimal::one(),
            }),
            MockQueryMsg::Route { denoms } => to_binary(&Route {
                key: format!("{0}|{1}", denoms[0].to_string(), denoms[1].to_string()),
                operations: vec![],
                decimal_delta: 0,
            }),
            MockQueryMsg::Simulation { offer_asset, ask } => {
                let offer = match offer_asset.info {
                    AssetInfo::NativeToken { denom } => denom.to_string(),
                };
                let price = *self.get_price(ask.to_string(), offer)
                                    .ok_or_else(|| StdError::generic_err("No price"))?;
                to_binary(&SimulationResponse {
                return_amount: Uint256::from(offer_asset.amount * price),
                spread_amount: Uint256::from(0u128),
                commission_amount: Uint256::from(0u128),
            })},
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MockQueryMsg {
    Config {},
    Pool {},
    Route { denoms: [Denom; 2] },
    Simulation { offer_asset: Asset, ask: Denom }
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.execute_query(&request)
    }
}
