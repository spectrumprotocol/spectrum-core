use cosmwasm_std::{OverflowError, StdError, Uint128};
use kujira::denom::Denom;
use thiserror::Error;

/// ## Description
/// This enum describes maker contract errors!
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid bridge {0} to {1}")]
    InvalidBridge(Denom, Denom),

    #[error("Invalid bridge destination. {0} cannot be swapped to ASTRO")]
    InvalidBridgeDestination(Denom),

    #[error("Max bridge length of {0} was reached")]
    MaxBridgeDepth(u64),

    #[error("Cannot swap {0}. No swap destinations")]
    CannotSwap(String),

    #[error("Incorrect max spread")]
    IncorrectMaxSpread {},

    #[error("Cannot collect. Remove duplicate asset")]
    DuplicatedAsset {},

    #[error("Assertion failed; minimum receive amount: {minimum_receive}, actual amount: {amount}")]
    AssertionMinimumReceive { minimum_receive: Uint128, amount: Uint128 },
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}

impl From<ContractError> for StdError {
    fn from(err: ContractError) -> Self {
        StdError::generic_err(format!("{}", err))
    }
}
