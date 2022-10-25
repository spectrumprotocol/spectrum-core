use astroport::asset::Asset;
use cosmwasm_std::{to_binary, Addr, CosmosMsg, Decimal, StdResult, Uint128, WasmMsg, StdError, Binary};
use cw20::{Cw20ReceiveMsg, Expiration, Logo};
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

    /// token info
    pub name: String,
    pub symbol: String,
}

/// This structure describes the execute messages available in the contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Receives a message of type [`Cw20ReceiveMsg`]
    Receive(Cw20ReceiveMsg),
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
        /// The list of asset to bond
        assets: Vec<Asset>,
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

    /// cw20
    /// Transfer is a base message to move tokens to another account without triggering actions
    Transfer { recipient: String, amount: Uint128 },
    /// Burn is a base message to destroy tokens forever
    Burn { amount: Uint128 },
    /// Send is a base message to transfer tokens to a contract and trigger an action
    /// on the receiving contract.
    Send {
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
    /// Only with "approval" extension. Allows spender to access an additional amount tokens
    /// from the owner's (env.sender) account. If expires is Some(), overwrites current allowance
    /// expiration with this one.
    IncreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    /// Only with "approval" extension. Lowers the spender's access of tokens
    /// from the owner's (env.sender) account by amount. If expires is Some(), overwrites current
    /// allowance expiration with this one.
    DecreaseAllowance {
        spender: String,
        amount: Uint128,
        expires: Option<Expiration>,
    },
    /// Only with "approval" extension. Transfers amount tokens from owner -> recipient
    /// if `env.sender` has sufficient pre-approval.
    TransferFrom {
        owner: String,
        recipient: String,
        amount: Uint128,
    },
    /// Only with "approval" extension. Sends amount tokens from owner -> contract
    /// if `env.sender` has sufficient pre-approval.
    SendFrom {
        owner: String,
        contract: String,
        amount: Uint128,
        msg: Binary,
    },
    /// Only with "approval" extension. Destroys tokens forever
    BurnFrom { owner: String, amount: Uint128 },
    /// Only with the "mintable" extension. If authorized, creates amount new tokens
    /// and adds to the recipient balance.
    Mint { recipient: String, amount: Uint128 },
    /// Only with the "marketing" extension. If authorized, updates marketing metadata.
    /// Setting None/null for any of these will leave it unchanged.
    /// Setting Some("") will clear this field on the contract storage
    UpdateMarketing {
        /// A URL pointing to the project behind this token.
        project: Option<String>,
        /// A longer description of the token and it's utility. Designed for tooltips or such
        description: Option<String>,
        /// The address (if any) who can update this data structure
        marketing: Option<String>,
    },
    /// If set as the "marketing" role on the contract, upload a new URL, SVG, or PNG for the token
    UploadLogo(Logo),
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
    pub fn into_cosmos_msg(&self, contract_addr: &Addr) -> StdResult<CosmosMsg> {
        Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from(contract_addr),
            msg: to_binary(&ExecuteMsg::Callback(self.clone()))?,
            funds: vec![],
        }))
    }
}

/// This structure describes custom hooks for the CW20.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    // Bond LP token
    Bond { staker_addr: Option<String> },
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

    /// cw20
    /// Returns the current balance of the given address, 0 if unset.
    /// Return type: BalanceResponse.
    Balance { address: String },
    /// Returns metadata on the contract - name, decimals, supply, etc.
    /// Return type: TokenInfoResponse.
    TokenInfo {},
    /// Only with "mintable" extension.
    /// Returns who can mint and the hard cap on maximum tokens after minting.
    /// Return type: MinterResponse.
    Minter {},
    /// Only with "allowance" extension.
    /// Returns how much spender can use from owner account, 0 if unset.
    /// Return type: AllowanceResponse.
    Allowance { owner: String, spender: String },
    /// Only with "enumerable" extension (and "allowances")
    /// Returns all allowances this owner has approved. Supports pagination.
    /// Return type: AllAllowancesResponse.
    AllAllowances {
        owner: String,
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "enumerable" extension
    /// Returns all accounts that have balances. Supports pagination.
    /// Return type: AllAccountsResponse.
    AllAccounts {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    /// Only with "marketing" extension
    /// Returns more metadata on the contract to display in the client:
    /// - description, logo, project url, etc.
    /// Return type: MarketingInfoResponse
    MarketingInfo {},
    /// Only with "marketing" extension
    /// Downloads the embedded logo data (if stored on chain). Errors if no logo data is stored for this
    /// contract.
    /// Return type: DownloadLogoResponse.
    DownloadLogo {},
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
    /// The weighted average deposit amount
    pub deposit_amount: Uint128,
    /// The weighted average deposit time
    pub deposit_time: u64,
}

/// This structure describes a migration message.
/// We currently take no arguments for migrations
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
    pub name: String,
    pub symbol: String,
}

fn is_valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || bytes.len() > 50 {
        return false;
    }
    true
}

fn is_valid_symbol(symbol: &str) -> bool {
    let bytes = symbol.as_bytes();
    if bytes.len() < 3 || bytes.len() > 12 {
        return false;
    }
    for byte in bytes.iter() {
        if (*byte != 45) && (*byte < 65 || *byte > 90) && (*byte < 97 || *byte > 122) {
            return false;
        }
    }
    true
}

impl InstantiateMsg {
    pub fn validate(&self) -> StdResult<()> {
        if !is_valid_name(&self.name) {
            return Err(StdError::generic_err(
                "Name is not in the expected format (3-50 UTF-8 bytes)",
            ));
        }
        if !is_valid_symbol(&self.symbol) {
            return Err(StdError::generic_err(
                "Ticker symbol is not in expected format [a-zA-Z\\-]{3,12}",
            ));
        }

        Ok(())
    }
}

impl MigrateMsg {
    pub fn validate(&self) -> StdResult<()> {
        if !is_valid_name(&self.name) {
            return Err(StdError::generic_err(
                "Name is not in the expected format (3-50 UTF-8 bytes)",
            ));
        }
        if !is_valid_symbol(&self.symbol) {
            return Err(StdError::generic_err(
                "Ticker symbol is not in expected format [a-zA-Z\\-]{3,12}",
            ));
        }

        Ok(())
    }
}