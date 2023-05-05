use cosmwasm_std::testing::{MockApi, MockStorage};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, BalanceResponse, BankQuery, Binary, Coin,
    ContractResult, Decimal, OwnedDeps, Querier, QuerierResult, QueryRequest, StdResult,
    SystemError, SystemResult, Uint128, WasmQuery, StdError, Uint256,
};
use kujira::asset::{Asset, AssetInfo};
use kujira::fin::SimulationResponse;
use std::collections::HashMap;

use kujira::denom::Denom;
use kujira::precision::Precision;
use kujira::query::{BankQuery as KujiraBankQuery, KujiraQuery, SupplyResponse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use spectrum::adapters::kujira::market_maker::{ConfigResponse, PoolResponse};
use spectrum::adapters::kujira::staking::StakeResponse;

pub fn mock_dependencies() -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier, KujiraQuery> {
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new();

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
        custom_query_type: Default::default(),
    }
}

pub struct WasmMockQuerier {
    supply: HashMap<String, Uint128>,
    staking_balances: HashMap<String, Uint128>,
    balances: HashMap<(String, String), Uint128>,
    rewards: Vec<Coin>,
    raw: HashMap<(String, Binary), Binary>,
    prices: HashMap<String, Decimal>, // price of offer asset
}

impl WasmMockQuerier {
    pub fn new() -> Self {
        WasmMockQuerier {
            supply: HashMap::new(),
            staking_balances: HashMap::new(),
            balances: HashMap::new(),
            rewards: vec![],
            prices: HashMap::new(),
            raw: HashMap::new(),
        }
    }

    pub fn set_supply(&mut self, denom: String, amount: Uint128) {
        self.supply.insert(denom, amount);
    }

    fn get_supply(&self, denom: String) -> Uint128 {
        *self.supply.get(&denom).unwrap_or(&Uint128::zero())
    }

    pub fn set_staking_balance(&mut self, token: String, amount: Uint128) {
        self.staking_balances.insert(token, amount);
    }

    fn get_staking_balance(&self, token: String) -> Uint128 {
        *self
            .staking_balances
            .get(&token)
            .unwrap_or(&Uint128::zero())
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

    pub fn set_rewards(&mut self, rewards: Vec<Coin>) {
        self.rewards = rewards;
    }

    fn get_rewards(&self) -> Vec<Coin> {
        self.rewards.to_vec()
    }

    pub fn set_price(&mut self, offer: String, price: Decimal) {
        self.prices.insert(offer, price);
    }

    fn get_price(&self, offer: String) -> Option<&Decimal> {
        self.prices.get(&offer)
    }

    fn execute_query(&self, request: &QueryRequest<KujiraQuery>) -> QuerierResult {
        let result = match request {
            QueryRequest::Custom(KujiraQuery::Bank {
                0: KujiraBankQuery::Supply { denom },
            }) => {
                let amount = self.get_supply(denom.to_string());
                to_binary(&SupplyResponse {
                    amount: Coin {
                        denom: denom.to_string(),
                        amount,
                    },
                })
            }
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
            MockQueryMsg::Stake { denom, addr } => {
                let amount = self.get_staking_balance(denom.clone());
                to_binary(&StakeResponse {
                    owner: addr,
                    denom: Denom::from(denom),
                    amount,
                })
            }
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
            MockQueryMsg::Fills { .. } => to_binary(&self.get_rewards()),
            MockQueryMsg::Simulation { offer_asset } => {
                let offer = match offer_asset.info {
                    AssetInfo::NativeToken { denom } => denom.to_string(),
                };
                let price = *self.get_price(offer)
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
    Stake { denom: String, addr: Addr },
    Config {},
    Pool {},
    Fills { denom: Denom, addr: Addr },
    Simulation { offer_asset: Asset },
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<KujiraQuery> = match from_slice(bin_request) {
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
