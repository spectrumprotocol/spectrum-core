use astroport::asset::Asset;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Decimal, StdResult, Uint128, WasmMsg};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: String,
    pub staking_contract: String,
    pub compound_proxy: String,
    pub controller: String,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub platform_fee_collector: String,
    pub community_fee_collector: String,
    pub controller_fee_collector: String,
    pub pair_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg), // Bond lp token
    // Update config
    UpdateConfig {
        controller: Option<String>,
        community_fee: Option<Decimal>,
        platform_fee: Option<Decimal>,
        controller_fee: Option<Decimal>,
        compound_proxy: Option<String>,
        platform_fee_collector: Option<String>,
        community_fee_collector: Option<String>,
        controller_fee_collector: Option<String>,
    },
    // Unbond lp token
    Unbond {
        amount: Uint128,
    },
    Compound {
        minimum_receive: Option<Uint128>,
    },
    // Bond asset with optimal swap
    BondAssets {
        assets: Vec<Asset>,
        minimum_receive: Option<Uint128>,
    },
    /// Creates a request to change the contract's ownership
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the proposal to change the owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    DropOwnershipProposal {},
    /// Claims contract ownership
    ClaimOwnership {},
    /// the callback of type [`CallbackMsg`]
    Callback(CallbackMsg),
}

/// ## Description
/// This structure describes the callback messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    Stake {
        prev_balance: Uint128,
        minimum_receive: Option<Uint128>,
    },
    BondTo {
        to: String,
        prev_balance: Uint128,
        minimum_receive: Option<Uint128>,
    },
}

// Modified from
// https://github.com/CosmWasm/cw-plus/blob/v0.8.0/packages/cw20/src/receiver.rs#L23
impl CallbackMsg {
    pub fn into_cosmos_msg(&self, contract_addr: &Addr) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from(contract_addr),
            msg: to_binary(&ExecuteMsg::Callback(self.clone()))?,
            funds: vec![],
        }))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Bond { staker_addr: Option<String> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // get config
    Config {},
    // get deposited balances
    RewardInfo { staker_addr: String },
    // get global state
    State {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: String,
    pub reward_info: RewardInfoResponseItem,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub staking_token: String,
    pub bond_amount: Uint128,
    pub bond_share: Uint128,
    pub deposit_amount: Uint128,
    pub deposit_time: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub total_bond_share: Uint128,
    pub earning: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
