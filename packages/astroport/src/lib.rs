pub mod asset;
pub mod common;
pub mod cosmwasm_ext;
pub mod factory;
pub mod generator;
pub mod generator_proxy;
pub mod maker;
pub mod oracle;
pub mod pair;
pub mod pair_bonded;
pub mod pair_concentrated;
pub mod querier;
pub mod router;
pub mod staking;
pub mod token;
pub mod vesting;
pub mod xastro_token;

#[cfg(test)]
mod mock_querier;
#[cfg(test)]
mod testing;

#[allow(clippy::all)]
mod uints {
    use uint::construct_uint;
    construct_uint! {
        pub struct U256(4);
    }
}

mod decimal_checked_ops {
    use cosmwasm_std::{Decimal, Fraction, OverflowError, Uint128, Uint256};
    use std::convert::TryInto;
    pub trait DecimalCheckedOps {
        fn checked_add(self, other: Decimal) -> Result<Decimal, OverflowError>;
        fn checked_mul_uint128(self, other: Uint128) -> Result<Uint128, OverflowError>;
    }

    impl DecimalCheckedOps for Decimal {
        fn checked_add(self, other: Decimal) -> Result<Decimal, OverflowError> {
            self.numerator()
                .checked_add(other.numerator())
                .map(|_| self + other)
        }
        fn checked_mul_uint128(self, other: Uint128) -> Result<Uint128, OverflowError> {
            if self.is_zero() || other.is_zero() {
                return Ok(Uint128::zero());
            }
            let multiply_ratio =
                other.full_mul(self.numerator()) / Uint256::from(self.denominator());
            if multiply_ratio > Uint256::from(Uint128::MAX) {
                Err(OverflowError::new(
                    cosmwasm_std::OverflowOperation::Mul,
                    self,
                    other,
                ))
            } else {
                Ok(multiply_ratio.try_into().unwrap())
            }
        }
    }
}

pub use decimal_checked_ops::DecimalCheckedOps;
pub use uints::U256;
