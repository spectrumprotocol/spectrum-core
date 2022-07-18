use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

/// ## Description
/// This enum describes pair contract errors!
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Invalid message")]
    InvalidMessage {},

    #[error("Cannot update owner")]
    CannotUpdateOwner {},

    #[error("Asset already registered")]
    AssetRegistered {},

    #[error("Cannot unbond more than balance")]
    UnbondExceedBalance {},

}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
