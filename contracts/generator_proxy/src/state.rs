use astroport::common::OwnershipProposal;
use cosmwasm_std::{Addr};
use cw_storage_plus::{Item, Map};
use crate::model::{Config, PoolInfo, RewardInfo, State, UserInfo};

/// Stores the contract config
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores pool info per LP token, key = LP token
pub const POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");   

/// Stores user info per user per LP token, key = LP token, User
pub const USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info");

/// Stores reward info per reward token, key = Reward Token
pub const REWARD_INFO: Map<&Addr, RewardInfo> = Map::new("reward_info");

/// Stores the contract state
pub const STATE: Item<State> = Item::new("state");

/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
