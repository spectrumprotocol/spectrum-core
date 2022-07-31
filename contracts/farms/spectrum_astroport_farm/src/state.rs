use astroport::asset::PairInfo;
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, Decimal, Uint128, Uint256};

use crate::ownership::OwnershipProposal;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub staking_contract: Addr,
    pub compound_proxy: Addr,
    pub controller: Addr,
    pub community_fee: Decimal,
    pub platform_fee: Decimal,
    pub controller_fee: Decimal,
    pub platform_fee_collector: Addr,
    pub community_fee_collector: Addr,
    pub controller_fee_collector: Addr,
    pub pair_info: PairInfo, // Store PairInfo instead
}

pub const CONFIG: Item<Config> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema)]
pub struct State {
    pub total_bond_share: Uint128,
    pub earning: Uint128,
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

    pub fn calc_user_balance(
        &self,
        lp_balance: Uint128,
        bond_share: Uint128,
        scaling_operation: ScalingOperation,
    ) -> Uint128 {
        if self.total_bond_share.is_zero() {
            Uint128::zero()
        } else {
            match scaling_operation {
                ScalingOperation::Truncate =>
                    lp_balance.multiply_ratio(bond_share, self.total_bond_share),
                ScalingOperation::Ceil => lp_balance
                    .multiply_ratio_and_ceil(bond_share, self.total_bond_share),
            }
        }
    }
}

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

/// ## Description
/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

pub enum ScalingOperation {
    Truncate,
    Ceil,
}

trait ScalingUint128 {
    fn multiply_ratio_and_ceil(
        &self,
        numerator: Uint128,
        denominator: Uint128,
    ) -> Uint128;
}

impl ScalingUint128 for Uint128 {
    /// Multiply Uint128 by Decimal, rounding up to the nearest integer.
    fn multiply_ratio_and_ceil(
        self: &Uint128,
        numerator: Uint128,
        denominator: Uint128,
    ) -> Uint128 {
        let x = self.full_mul(numerator);
        let y: Uint256 = denominator.into();
        ((x + y - Uint256::from(1u64)) / y).try_into().expect("multiplication overflow")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiply_ratio_and_ceil() {
        let a = Uint128::new(124);
        let b = a
            .multiply_ratio_and_ceil(Uint128::new(1), Uint128::new(3));
        assert_eq!(b, Uint128::new(42));

        let a = Uint128::new(123);
        let b = a
            .multiply_ratio_and_ceil(Uint128::new(1), Uint128::new(3));
        assert_eq!(b, Uint128::new(41));
    }
}
