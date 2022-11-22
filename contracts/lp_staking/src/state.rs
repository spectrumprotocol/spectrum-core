use cw_storage_plus::{Item, Map, Bound};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal, StdResult, Storage, Uint128, Addr, Deps, Order};

use crate::ownership::OwnershipProposal;

pub const CONFIG: Item<Config> = Item::new("config");
pub const STATE: Item<State> = Item::new("state");
pub const REWARD_INFOS: Map<&Addr, RewardInfo> = Map::new("reward_infos");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub reward_token: Addr,
    pub staking_token: Addr,
    pub distribution_schedule: Vec<(u64, u64, Uint128)>,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub last_distributed: u64,
    pub total_bond_amount: Uint128,
    pub global_reward_index: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfo {
    pub reward_index: Decimal,
    pub bond_amount: Uint128,
    pub pending_reward: Uint128,
}

/// returns rewards owned by this owner
pub fn read_reward_info(storage: &dyn Storage, owner: &Addr) -> StdResult<RewardInfo> {
    match REWARD_INFOS.may_load(storage, owner)? {
        Some(reward_info) => Ok(reward_info),
        None => Ok(RewardInfo {
            reward_index: Decimal::zero(),
            bond_amount: Uint128::zero(),
            pending_reward: Uint128::zero(),
        }),
    }
}

const DEFAULT_LIMIT: u32 = 10;
pub fn query_rewards(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<(Addr, RewardInfo)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT) as usize;
    let user_addr = if let Some(start_after) = start_after.clone() {
        deps.api.addr_validate(&start_after)?
    } else {
        Addr::unchecked("")
    };
    let start = if start_after.is_some() {
        Some(Bound::exclusive(&user_addr))
    } else {
        None
    };

    REWARD_INFOS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .collect::<StdResult<Vec<(Addr, RewardInfo)>>>()
}

/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");