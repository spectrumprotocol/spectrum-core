use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Uint128};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub spectrum_gov: Addr,
    pub astroport_generator: Addr,
    pub astro_token: Addr,
    pub compound_proxy: Addr,
    pub platform: Addr,
    pub controller: Addr,
    pub base_denom: String,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub platform_fee_collector: Addr,
    pub community_fee_collector: Addr,
    pub controller_fee_collector: Addr,
    pub pair_contract: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct State {
    pub earning: Uint128,
}

pub const STATE: Item<State> = Item::new("state");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub asset_token: Addr,
    pub staking_token: Addr,
    pub total_bond_share: Uint128,
}

impl PoolInfo {
    pub fn calc_bond_share(&self, bond_amount: Uint128, lp_balance: Uint128) -> Uint128 {
        if self.total_bond_share.is_zero() || lp_balance.is_zero() {
            bond_amount
        } else {
            bond_amount.multiply_ratio(self.total_bond_share, lp_balance)
        }
    }


    pub fn calc_user_balance(&self, lp_balance: Uint128, bond_share: Uint128) -> Uint128 {
        if self.total_bond_share.is_zero() {
            Uint128::zero()
        } else {
            lp_balance
                .multiply_ratio(bond_share, self.total_bond_share)
        }
    }
}


pub const POOL_INFO: Item<PoolInfo> = Item::new("pool_info");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfo {
    pub bond_share: Uint128,
    pub deposit_amount: Uint128,
    pub deposit_cost: Uint128,
    pub deposit_time: u64,
}

pub const REWARD: Map<&Addr, RewardInfo> = Map::new("reward");

impl RewardInfo {
    pub fn create() -> RewardInfo {
        RewardInfo {
            bond_share: Uint128::zero(),
            deposit_cost: Uint128::zero(),
            deposit_amount: Uint128::zero(),
            deposit_time: 0u64,
        }
    }
}
