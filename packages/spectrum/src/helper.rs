use cosmwasm_std::{Uint128, Uint256};

pub trait ScalingUint128 {
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
