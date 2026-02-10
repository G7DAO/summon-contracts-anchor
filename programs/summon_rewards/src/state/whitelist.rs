use anchor_lang::prelude::*;

use super::RewardType;

/// Entry in the token whitelist.
/// Maps from RewardsState: whitelistedTokens + tokenTypes mappings.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct WhitelistEntry {
    /// The SPL token mint address
    pub mint: Pubkey,
    /// The reward type classification
    pub reward_type: RewardType,
    /// Whether this entry is active
    pub is_active: bool,
}

/// Treasury token whitelist account.
/// Stores all whitelisted token mints and their types.
///
/// PDA seeds: ["whitelist", config.key()]
#[account]
pub struct TokenWhitelist {
    /// List of whitelisted token entries
    pub tokens: Vec<WhitelistEntry>,
    /// PDA bump seed
    pub bump: u8,
}

impl TokenWhitelist {
    /// Calculate space for a given max number of whitelist entries.
    pub fn space(max_entries: usize) -> usize {
        8 // discriminator
        + 4 // vec length prefix
        + max_entries * (32 + 1 + 1) // WhitelistEntry: Pubkey + RewardType enum + bool
        + 1 // bump
    }
}
