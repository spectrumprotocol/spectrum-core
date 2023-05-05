use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Coin, Decimal, Env, StdResult, Uint128};
use kujira::denom::Denom;
use kujira::query::SupplyResponse;
use spectrum::adapters::kujira::market_maker::{MarketMaker, PoolResponse};
use spectrum::adapters::kujira::staking::Staking;
use spectrum::compound_proxy::Compounder;
use spectrum::helper::{compute_deposit_time, ScalingUint128};
use spectrum::router::Router;
use crate::error::ContractError;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub staking: Staking,
    pub compound_proxy: Compounder,
    pub controller: Addr,
    pub fee: Decimal,
    pub fee_collector: Addr,
    pub router: Router,
}

pub const CONFIG: Item<Config> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub market_maker: MarketMaker,
    pub denoms: [Denom; 2],
    pub rewards: Vec<Coin>,
}

impl PoolInfo {
    pub fn get_clp_name(&self, env: &Env) -> String {
        format!("factory/{0}/{1}", env.contract.address, self.market_maker.0)
    }
}

pub const POOL: Map<&Addr, PoolInfo> = Map::new("pool");

pub trait SupplyResponseEx {
    fn calc_bond_share(
        &self,
        bond_amount: Uint128,
        lp_balance: Uint128,
        scaling_operation: ScalingOperation,
    ) -> Uint128;

    fn calc_bond_amount(
        &self,
        lp_balance: Uint128,
        bond_share: Uint128,
    ) -> Uint128;
}

impl SupplyResponseEx for SupplyResponse {
    fn calc_bond_share(
        &self,
        bond_amount: Uint128,
        lp_balance: Uint128,
        scaling_operation: ScalingOperation,
    ) -> Uint128 {
        if self.amount.amount.is_zero() || lp_balance.is_zero() {
            bond_amount
        } else {
            match scaling_operation {
                ScalingOperation::Truncate =>
                    bond_amount.multiply_ratio(self.amount.amount, lp_balance),
                ScalingOperation::Ceil => bond_amount
                    .multiply_ratio_and_ceil(self.amount.amount, lp_balance),
            }
        }
    }

    fn calc_bond_amount(
        &self,
        lp_balance: Uint128,
        bond_share: Uint128,
    ) -> Uint128 {
        if self.amount.amount.is_zero() {
            Uint128::zero()
        } else {
            lp_balance.multiply_ratio(bond_share, self.amount.amount)
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct RewardInfo {
    pub deposit_amount: Uint128,
    pub deposit_time: u64,

    pub deposit_share: Uint128,
    pub deposit_costs: [Uint128; 2],
}

impl RewardInfo {

    pub fn bond(&mut self, bond_share: Uint128, deposit_amount: Uint128, time: u64, pool_info: &PoolResponse, lp_supply: Uint128) -> StdResult<()> {
        self.deposit_share += bond_share;
        let last_deposit_amount = self.deposit_amount;
        self.deposit_amount += deposit_amount;
        self.deposit_time = compute_deposit_time(
            last_deposit_amount,
            deposit_amount,
            self.deposit_time,
            time,
        )?;
        self.deposit_costs[0] += pool_info.balances[0].multiply_ratio(deposit_amount, lp_supply);
        self.deposit_costs[1] += pool_info.balances[1].multiply_ratio(deposit_amount, lp_supply);

        Ok(())
    }

    pub fn unbond(&mut self, bond_share: Uint128) -> StdResult<()> {
        let old_total_share = self.deposit_share;
        self.deposit_share = self.deposit_share.checked_sub(bond_share)?;
        self.deposit_amount = self.deposit_amount
            .multiply_ratio(self.deposit_share, old_total_share);
        self.deposit_costs[0] = self.deposit_costs[0].multiply_ratio(self.deposit_share, old_total_share);
        self.deposit_costs[1] = self.deposit_costs[1].multiply_ratio(self.deposit_share, old_total_share);

        Ok(())
    }
}

pub const REWARD: Map<(&Addr, &Addr), RewardInfo> = Map::new("reward");

const DAY: u64 = 86400;

impl RewardInfo {
    pub fn limit_user_lp(&self, amount: Uint128, time: u64) -> Uint128 {
        let deposit_time = time - self.deposit_time;
        if deposit_time < DAY && amount > self.deposit_amount {
            self.deposit_amount + (amount - self.deposit_amount).multiply_ratio(deposit_time, DAY)
        } else {
            amount
        }
    }
}

pub enum ScalingOperation {
    Truncate,
    Ceil,
}

pub fn extract_market_maker_from_lp(denom: &String) -> Result<Addr, ContractError> {
    if denom.starts_with("factory/") && denom.ends_with("/ulp") {
        let addr = &denom[8..(denom.len() - 4)];
        Ok(Addr::unchecked(addr))
    } else {
        Err(ContractError::InvalidFunds {})
    }
}

pub fn extract_market_maker_from_clp(denom: &str, env: &Env) -> Result<Addr, ContractError> {
    let prefix = format!("factory/{0}/", env.contract.address);
    if denom.starts_with(&prefix) {
        let addr = &denom[prefix.len()..];
        Ok(Addr::unchecked(addr))
    } else {
        Err(ContractError::InvalidFunds {})
    }
}
