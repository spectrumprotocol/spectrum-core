use cosmwasm_std::{to_binary, Addr, CosmosMsg, Decimal, StdResult, Uint128, WasmMsg};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// This structure describes the parameters for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// The owner address
    pub owner: String,
    /// The LP staking generator contract address
    pub staking_contract: String,
    /// The compound proxy contract address
    pub compound_proxy: String,
    /// The controller address to execute compound
    pub controller: String,
    /// The performance fee
    pub fee: Decimal,
    /// The fee collector contract address
    pub fee_collector: String,
    /// The LP token contract address
    pub liquidity_token: String,
    /// the base reward token contract address
    pub base_reward_token: String,
    /// The pair contract address
    pub pair: String,

    /// token info
    pub name: String,
    pub symbol: String,
}

/// This structure describes the execute messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    // Bond LP token
    Bond { staker_addr: Option<String> },
    /// Update contract config
    UpdateConfig {
        /// The compound proxy contract address
        compound_proxy: Option<String>,
        /// The controller address
        controller: Option<String>,
        /// The performance fee
        fee: Option<Decimal>,
        /// The fee collector contract address
        fee_collector: Option<String>,
    },
    /// Unbond LP token
    Unbond {
        /// The LP amount to unbond
        amount: Uint128,
    },
    /// Compound LP rewards
    Compound {
        /// The minimum expected amount of LP token
        minimum_receive: Option<Uint128>,
        /// Slippage tolerance when providing LP
        slippage_tolerance: Option<Decimal>,
    },
    /// Bond asset with optimal swap
    BondAssets {
        /// The minimum expected amount of LP token
        minimum_receive: Option<Uint128>,
        /// The flag to skip optimal swap
        no_swap: Option<bool>,
        /// Slippage tolerance when providing LP
        slippage_tolerance: Option<Decimal>,
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
    /// The callback of type [`CallbackMsg`]
    Callback(CallbackMsg),
}

/// This structure describes the callback messages of the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallbackMsg {
    Stake {
        /// The previous LP balance in the contract
        prev_balance: Uint128,
        /// The minimum expected amount of LP token
        minimum_receive: Option<Uint128>,
    },
    BondTo {
        /// The address to bond LP
        to: Addr,
        /// The previous LP balance in the contract
        prev_balance: Uint128,
        /// The minimum expected amount of LP token
        minimum_receive: Option<Uint128>,
    },
}

// Modified from
// https://github.com/CosmWasm/cw-plus/blob/v0.8.0/packages/cw20/src/receiver.rs#L23
impl CallbackMsg {
    pub fn to_cosmos_msg(&self, contract_addr: &Addr) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from(contract_addr),
            msg: to_binary(&ExecuteMsg::Callback(self.clone()))?,
            funds: vec![],
        }))
    }
}

/// This structure describes query messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Returns the contract config
    Config {},
    /// Returns the deposited balances
    RewardInfo { staker_addr: String },
    /// Returns the global state
    State {},
}

/// This structure holds the parameters for reward info query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponse {
    /// The staker address
    pub staker_addr: String,
    /// The detail on reward info
    pub reward_info: RewardInfoResponseItem,
}

/// This structure holds the detail for reward info
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInfoResponseItem {
    /// The LP token contract address
    pub staking_token: String,
    /// The LP token amount bonded
    pub bond_amount: Uint128,
    /// The share of total LP token bonded
    pub bond_share: Uint128,
    /// The deposit amount
    pub deposit_amount: Uint128,
    /// The weighted average deposit time
    pub deposit_time: u64,
    /// The deposit cost
    pub deposit_costs: Vec<Uint128>,
}
