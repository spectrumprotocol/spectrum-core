use cosmwasm_std::{Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigInfo {
    pub owner: String,
    pub spectrum_gov: String,
    pub astroport_generator: String,
    pub astro_token: String,
    pub compound_proxy: String,
    pub platform: String,
    pub controller: String,
    pub base_denom: String,
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
        owner: Option<String>,
        controller: Option<String>,
        community_fee: Option<Decimal>,
        platform_fee: Option<Decimal>,
        controller_fee: Option<Decimal>,
    },
    // Unbond lp token
    Unbond {
        amount: Uint128,
    },
    RegisterAsset {
        asset_token: String,
        staking_token: String,
    },
    Compound {},
    SendFee {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Bond {
        staker_addr: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // get config
    Config {}, 
    // get vault setting
    Pool {},
    // get deposited balances
    RewardInfo { staker_addr: String },
    // get global state
    State {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolResponse {
    pub pool: PoolItem,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolItem {
    pub asset_token: String, // asset token
    pub staking_token: String, // LP staking token
    pub total_bond_share: Uint128, // total share
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    pub staker_addr: String,
    pub reward_info: RewardInfoResponseItem,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    pub asset_token: String,
    pub bond_amount: Uint128,
    pub bond_share: Uint128,
    pub deposit_amount: Option<Uint128>,
    pub deposit_time: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
pub struct StateInfo {
    pub earning: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
