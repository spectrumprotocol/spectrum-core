use cosmwasm_std::{OverflowError, StdError, Uint128};
use thiserror::Error;

/// ## Description
/// This enum describes pair contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid asset")]
    InvalidAsset {},

    #[error("Invalid funds")]
    InvalidFunds {},

    #[error("Invalid pair")]
    InvalidPair {},

    #[error("Invalid operations")]
    InvalidOperations {},

    #[error("Duplicated assets in asset infos")]
    DuplicatedAssets {},

    #[error("Must provide at least 2 assets!")]
    MustProvideNAssets {},

    #[error("The limit exceeded of swap assets!")]
    SwapLimitExceeded {},

    #[error("Assertion failed; minimum receive amount: {receive}, swap amount: {amount}")]
    AssertionMinimumReceive { receive: Uint128, amount: Uint128 },

    #[error("Invalid zero amount")]
    InvalidZeroAmount {},
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
