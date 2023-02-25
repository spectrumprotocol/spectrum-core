use cosmwasm_std::{attr, Addr, DepsMut, Env, MessageInfo, Response, StdError, StdResult, Storage, CustomQuery};
use cw_storage_plus::Item;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// ## Description
/// This structure describes the basic settings for creating a request for a change of ownership.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OwnershipProposal {
    /// a new ownership.
    pub owner: Addr,
    /// time to live a request
    pub ttl: u64,
}

/// ## Description
/// Creates a new request to change ownership. Returns an [`Err`] on failure or returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Executor
/// Only owner can execute it
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `env` is the object of type [`Env`].
///
/// `new_owner` is a new owner.
///
/// `expires_in` is the validity period of the offer to change the owner.
///
/// `owner` is the current owner.
///
/// `proposal` is the object of type [`OwnershipProposal`].
pub fn propose_new_owner<C: CustomQuery, T>(
    deps: DepsMut<C>,
    info: MessageInfo,
    env: Env,
    new_owner: String,
    expires_in: u64,
    owner: Addr,
) -> StdResult<Response<T>> {
    // permission check
    if info.sender != owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    let new_owner = deps.api.addr_validate(&new_owner)?;

    // check that owner is not the same
    if new_owner == owner {
        return Err(StdError::generic_err("New owner cannot be same"));
    }

    OWNERSHIP_PROPOSAL.save(
        deps.storage,
        &OwnershipProposal {
            owner: new_owner.clone(),
            ttl: env.block.time.seconds() + expires_in,
        },
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "propose_new_owner"),
        attr("new_owner", new_owner),
    ]))
}

/// ## Description
/// Removes a request to change ownership. Returns an [`Err`] on failure or returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Executor
/// Only owner can execute it
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `owner` is the current owner.
///
/// `proposal` is the object of type [`OwnershipProposal`].
pub fn drop_ownership_proposal<C: CustomQuery, T>(
    deps: DepsMut<C>,
    info: MessageInfo,
    owner: Addr,
) -> StdResult<Response<T>> {
    // permission check
    if info.sender != owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    OWNERSHIP_PROPOSAL.remove(deps.storage);

    Ok(Response::new().add_attributes(vec![attr("action", "drop_ownership_proposal")]))
}

/// ## Description
/// Approves owner. Returns an [`Err`] on failure or returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Executor
/// Only owner can execute it
/// ## Params
/// `deps` is the object of type [`DepsMut`].
///
/// `info` is the object of type [`MessageInfo`].
///
/// `env` is the object of type [`Env`].
///
/// `proposal` is the object of type [`OwnershipProposal`].
///
/// `cb` is a type of callback function that takes two parameters of type [`DepsMut`] and [`Addr`].
pub fn claim_ownership<T>(
    storage: &mut dyn Storage,
    info: MessageInfo,
    env: Env,
) -> StdResult<Response<T>> {
    let p: OwnershipProposal = OWNERSHIP_PROPOSAL
        .load(storage)
        .map_err(|_| StdError::generic_err("Ownership proposal not found"))?;

    // Check sender
    if info.sender != p.owner {
        return Err(StdError::generic_err("Unauthorized"));
    }

    if env.block.time.seconds() > p.ttl {
        return Err(StdError::generic_err("Ownership proposal expired"));
    }

    OWNERSHIP_PROPOSAL.remove(storage);

    Ok(Response::new().add_attributes(vec![
        attr("action", "claim_ownership"),
        attr("new_owner", p.owner),
    ]))
}
