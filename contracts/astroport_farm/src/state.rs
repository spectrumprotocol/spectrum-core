use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, StdResult, Storage, Uint128};
use cw20::AllowanceResponse;
use astroport::pair::PoolResponse;
use spectrum::adapters::generator::Generator;
use spectrum::adapters::pair::Pair;
use spectrum::compound_proxy::Compounder;
use spectrum::helper::{compute_deposit_time, ScalingUint128};

use crate::ownership::OwnershipProposal;

pub fn default_pair() -> Pair {
    Pair(Addr::unchecked(""))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub staking_contract: Generator,
    pub compound_proxy: Compounder,
    pub controller: Addr,
    pub fee: Decimal,
    pub fee_collector: Addr,
    pub liquidity_token: Addr,
    pub base_reward_token: Addr,

    /// token info
    #[serde(default)] pub name: String,
    #[serde(default)] pub symbol: String,
    #[serde(default = "default_pair")] pub pair: Pair,
}

pub const CONFIG: Item<Config> = Item::new("config");

#[derive(Serialize, Deserialize, Clone,Debug, PartialEq, JsonSchema)]
pub struct State {
    pub total_bond_share: Uint128,
}

pub const STATE: Item<State> = Item::new("state");

impl State {
    pub fn calc_bond_share(
        &self,
        bond_amount: Uint128,
        lp_balance: Uint128,
        scaling_operation: ScalingOperation,
    ) -> Uint128 {
        if self.total_bond_share.is_zero() || lp_balance.is_zero() {
            bond_amount
        } else {
            match scaling_operation {
                ScalingOperation::Truncate =>
                    bond_amount.multiply_ratio(self.total_bond_share, lp_balance),
                ScalingOperation::Ceil => bond_amount
                    .multiply_ratio_and_ceil(self.total_bond_share, lp_balance),
            }
        }
    }

    pub fn calc_bond_amount(
        &self,
        lp_balance: Uint128,
        bond_share: Uint128,
    ) -> Uint128 {
        if self.total_bond_share.is_zero() {
            Uint128::zero()
        } else {
            lp_balance.multiply_ratio(bond_share, self.total_bond_share)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct RewardInfo {
    pub bond_share: Uint128,
    pub deposit_amount: Uint128,
    pub deposit_time: u64,

    #[serde(default)] pub transfer_share: Uint128,
    #[serde(default)] pub deposit_costs: Vec<Uint128>,
}

impl RewardInfo {
    pub fn ensure_deposit_costs(&mut self, storage: &dyn Storage) -> StdResult<()> {
        if !self.deposit_amount.is_zero() && self.deposit_costs.is_empty() {
            let pool_info = POOL_INFO.load(storage)?;
            self.deposit_costs = pool_info.assets.into_iter()
                .map(|it| it.amount.multiply_ratio(self.deposit_amount, pool_info.total_share))
                .collect();
        }
        Ok(())
    }

    pub fn bond(&mut self, bond_share: Uint128, deposit_amount: Uint128, time: u64, pool_info: &PoolResponse) -> StdResult<()> {
        self.bond_share += bond_share;
        let last_deposit_amount = self.deposit_amount;
        self.deposit_amount += deposit_amount;
        self.deposit_time = compute_deposit_time(
            last_deposit_amount,
            deposit_amount,
            self.deposit_time,
            time,
        )?;
        for (i, asset) in pool_info.assets.iter().enumerate() {
            if self.deposit_costs.len() == i {
                self.deposit_costs.push(Uint128::zero());
            }
            self.deposit_costs[i] += asset.amount.multiply_ratio(deposit_amount, pool_info.total_share);
        }

        Ok(())
    }

    pub fn unbond(&mut self, bond_share: Uint128) -> StdResult<()> {
        let old_total_share = self.bond_share + self.transfer_share;
        self.bond_share = self.bond_share.checked_sub(bond_share)?;
        let total_share = self.bond_share + self.transfer_share;
        self.deposit_amount = self.deposit_amount
            .multiply_ratio(total_share, old_total_share);
        self.deposit_costs = self.deposit_costs.iter()
            .map(|it| it.multiply_ratio(total_share, old_total_share))
            .collect();

        Ok(())
    }
}

pub const REWARD: Map<&Addr, RewardInfo> = Map::new("reward");

const DAY: u64 = 86400;

impl RewardInfo {
    pub fn calc_user_balance(&self, state: &State, lp_balance: Uint128, time: u64) -> Uint128 {
        let amount = state.calc_bond_amount(lp_balance, self.bond_share);
        let deposit_time = time - self.deposit_time;
        if deposit_time < DAY && amount > self.deposit_amount {
            self.deposit_amount + (amount - self.deposit_amount).multiply_ratio(deposit_time, DAY)
        } else {
            amount
        }
    }
}

/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

pub enum ScalingOperation {
    Truncate,
    Ceil,
}

pub const ALLOWANCES: Map<(&Addr, &Addr), AllowanceResponse> = Map::new("allowance");
pub const POOL_INFO: Item<PoolResponse> = Item::new("pool_info");
