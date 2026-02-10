use anchor_lang::prelude::*;

use crate::state::RewardType;

/// Emitted when the program is initialized.
#[event]
pub struct ProgramInitialized {
    pub admin: Pubkey,
    pub manager: Pubkey,
    pub minter: Pubkey,
    pub dev_config: Pubkey,
}

/// Emitted when a new reward token is created.
/// Maps from Rewards.sol: TokenAdded event.
#[event]
pub struct RewardTokenCreated {
    pub token_id: u64,
    pub max_supply: u64,
    pub token_uri: String,
}

/// Emitted when an access token is minted to a user.
/// Maps from Rewards.sol: Minted event.
#[event]
pub struct Minted {
    pub to: Pubkey,
    pub token_id: u64,
    pub amount: u64,
    pub soulbound: bool,
}

/// Emitted when a user claims (burns access token + receives rewards).
/// Maps from Rewards.sol: Claimed event.
#[event]
pub struct Claimed {
    pub to: Pubkey,
    pub token_id: u64,
    pub amount: u64,
}

/// Emitted when a token is whitelisted for treasury.
/// Maps from RewardsState.sol: TokenWhitelisted event.
#[event]
pub struct TokenWhitelisted {
    pub mint: Pubkey,
    pub reward_type: RewardType,
}

/// Emitted when a token is removed from the whitelist.
/// Maps from RewardsState.sol: TokenRemovedFromWhitelist event.
#[event]
pub struct TokenRemovedFromWhitelist {
    pub mint: Pubkey,
}

/// Emitted when tokens are deposited to the treasury.
/// Maps from Treasury.sol: TreasuryDeposit event.
#[event]
pub struct TreasuryDeposit {
    pub mint: Pubkey,
    pub amount: u64,
}

/// Emitted when the reward supply is changed.
/// Maps from Rewards.sol: RewardSupplyChanged event.
#[event]
pub struct RewardSupplyChanged {
    pub token_id: u64,
    pub old_supply: u64,
    pub new_supply: u64,
}

/// Emitted when a token URI is updated.
/// Maps from Rewards.sol: TokenURIChanged event.
#[event]
pub struct TokenUriChanged {
    pub token_id: u64,
    pub new_uri: String,
}

/// Emitted when token mint pause status is updated.
#[event]
pub struct TokenMintPausedUpdated {
    pub token_id: u64,
    pub is_paused: bool,
}

/// Emitted when token claim pause status is updated.
#[event]
pub struct ClaimRewardPausedUpdated {
    pub token_id: u64,
    pub is_paused: bool,
}

/// Emitted when a whitelist signer is added.
#[event]
pub struct WhitelistSignerAdded {
    pub signer: Pubkey,
}

/// Emitted when a whitelist signer is removed.
#[event]
pub struct WhitelistSignerRemoved {
    pub signer: Pubkey,
}

/// Emitted when a user nonce is consumed.
/// Maps from RewardsState.sol: UserNonceUsed event.
#[event]
pub struct UserNonceUsed {
    pub user: Pubkey,
    pub nonce: u64,
}

/// Emitted when a role is updated.
#[event]
pub struct RoleUpdated {
    pub role: String,
    pub old_address: Pubkey,
    pub new_address: Pubkey,
}
