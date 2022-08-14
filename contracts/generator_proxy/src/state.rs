use cosmwasm_std::{Addr};
use cw_storage_plus::{Item, Map};
use crate::model::{Config, PoolInfo, RewardInfo, State, UserInfo};

pub const CONFIG: Item<Config> = Item::new("config");
pub const POOL_INFO: Map<&Addr, PoolInfo> = Map::new("pool_info");          // key = LP
pub const USER_INFO: Map<(&Addr, &Addr), UserInfo> = Map::new("user_info"); // key = LP, user
pub const REWARD_INFO: Map<&Addr, RewardInfo> = Map::new("reward_info");    // key = Token
pub const STATE: Item<State> = Item::new("state");
