use cosmwasm_std::{Addr, CosmosMsg, Decimal, StdResult, to_binary, Uint128, WasmMsg};
use cw20::{Cw20ReceiveMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use astroport::restricted_vector::RestrictedVector;
use spectrum::adapters::generator::Generator;
use spectrum::helper::ScalingUint128;
use crate::astro_gov::{AstroGov, AstroGovUnchecked};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub generator: String,
    pub astro_gov: AstroGovUnchecked,
    pub owner: String,
    pub controller: String,
    pub astro_token: String,
    pub fee_distributor: String,
    pub income_distributor: String,
    pub max_quota: Uint128,
    pub staker_rate: Decimal,
    pub boost_fee: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub generator: Generator,
    pub astro_gov: AstroGov,
    pub owner: Addr,
    pub controller: Addr,
    pub astro_token: Addr,
    pub fee_distributor: Addr,
    pub income_distributor: Addr,
    pub max_quota: Uint128,
    pub staker_rate: Decimal,
    pub boost_fee: Decimal,
}

pub fn zero_address() -> Addr {
    Addr::unchecked("")
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct PoolInfo {
    pub total_bond_share: Uint128,
    pub reward_indexes: RestrictedVector<Addr, Decimal>,
    pub prev_reward_user_index: Decimal,
    pub prev_reward_debt_proxy: RestrictedVector<Addr, Uint128>,
}

impl PoolInfo {
    pub fn calc_bond_share(
        &self,
        total_bond_amount: Uint128,
        amount: Uint128,
        ceiling: bool,
    ) -> Uint128 {
        if self.total_bond_share.is_zero() || total_bond_amount.is_zero() {
            amount
        } else if ceiling {
            amount.multiply_ratio_and_ceil(self.total_bond_share, total_bond_amount)
        } else {
            amount.multiply_ratio(self.total_bond_share, total_bond_amount)
        }
    }

    pub fn calc_bond_amount(&self, total_bond_amount: Uint128, share: Uint128) -> Uint128 {
        if self.total_bond_share.is_zero() {
            Uint128::zero()
        } else {
            total_bond_amount.multiply_ratio(share, self.total_bond_share)
        }
    }

}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UserInfo {
    pub bond_share: Uint128,
    pub reward_indexes: RestrictedVector<Addr, Decimal>,
    pub pending_rewards: RestrictedVector<Addr, Uint128>,
}

impl UserInfo {
    pub fn create(pool_info: &PoolInfo) -> UserInfo {
        UserInfo {
            bond_share: Uint128::zero(),
            reward_indexes: pool_info.reward_indexes.clone(),
            pending_rewards: RestrictedVector::default(),
        }
    }
}


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LockedIncome {
    pub start: u64,
    pub end: u64,
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct RewardInfo {
    pub reconciled_amount: Uint128,
    pub fee: Uint128,
    pub staker_income: Uint128,
    pub locked_income: Option<LockedIncome>,
}

impl RewardInfo {
    pub fn realize_unlocked_amount(
        &mut self,
        now: u64,
    ) {
        if let Some(locked_income) = &self.locked_income {
            if now >= locked_income.end {
                self.staker_income += locked_income.amount;
                self.locked_income = None;
            } else if now > locked_income.start {
                let unlocked_amount = locked_income.amount.multiply_ratio(
                    now - locked_income.start,
                    locked_income.end - locked_income.start,
                );
                self.staker_income += unlocked_amount;
                self.locked_income = Some(LockedIncome {
                    start: now,
                    end: locked_income.end,
                    amount: locked_income.amount - unlocked_amount,
                });
            }
        }
    }

}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub next_claim_period: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Receive(Cw20ReceiveMsg),
    Callback(CallbackMsg),

    // config
    UpdateConfig {
        controller: Option<String>,
        boost_fee: Option<Decimal>,
    },

    // controller's actions
    UpdateParameters {
        max_quota: Option<Uint128>,
        staker_rate: Option<Decimal>,
    },

    // ControllerVote {
    //     votes: Vec<(String, u16)>,
    // },
    // ExtendLockTime { time: u64 },
    // SendIncome {},
    //
    // // anyone
    // ReconcileGovIncome {},

    // from generator
    /// Update rewards and return it to user.
    ClaimRewards {
        /// the LP token contract address
        lp_tokens: Vec<String>,
    },
    /// Withdraw LP tokens from the Generator
    Withdraw {
        /// The address of the LP token to withdraw
        lp_token: String,
        /// The amount to withdraw
        amount: Uint128,
    },
    /// Creates a request to change the contract's ownership
    ProposeNewOwner {
        /// The newly proposed owner
        owner: String,
        /// The validity period of the proposal to change the owner
        expires_in: u64,
    },
    /// Removes a request to change contract ownership
    DropOwnershipProposal {},
    /// Claims contract ownership
    ClaimOwnership {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    AfterClaimed {
        lp_token: Addr,
    },
    Deposit {
        lp_token: Addr,
        staker_addr: Addr,
        amount: Uint128,
    },
    Withdraw {
        lp_token: Addr,
        staker_addr: Addr,
        amount: Uint128,
    },
    AfterBondChanged {
        lp_token: Addr,
    },
    ClaimRewards {
        lp_token: Addr,
        staker_addr: Addr,
    },
}

impl CallbackMsg {
    pub fn to_cosmos_msg(&self, contract_addr: &Addr) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from(contract_addr),
            msg: to_binary(&ExecuteMsg::Callback(self.clone()))?,
            funds: vec![],
        }))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    // from generator
    Deposit {},

    // ASTRO staking
    // Stake {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    PoolInfo {
        lp_token: String,
    },
    UserInfo {
        lp_token: String,
        user: String,
    },
    RewardInfo {
        token: String,
    },
    State {},

    // from generator
    PendingToken { lp_token: String, user: String },
    Deposit { lp_token: String, user: String },
}
