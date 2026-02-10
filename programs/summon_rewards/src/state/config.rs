use anchor_lang::prelude::*;

/// Global configuration account for the Summon Rewards program.
/// Maps to the combined role management from Rewards.sol AccessControl roles.
///
/// PDA seeds: ["config"]
#[account]
pub struct RewardsConfig {
    /// DEFAULT_ADMIN equivalent - can update other roles
    pub admin: Pubkey,
    /// MANAGER_ROLE - can create tokens, pause, manage treasury
    pub manager: Pubkey,
    /// MINTER_ROLE - can admin mint reward tokens
    pub minter: Pubkey,
    /// DEV_CONFIG_ROLE - can manage whitelist signers
    pub dev_config: Pubkey,
    /// Global pause flag (Pausable equivalent)
    pub is_paused: bool,
    /// Counter for total reward tokens created
    pub reward_token_count: u64,
    /// PDA bump seed
    pub bump: u8,
}

impl RewardsConfig {
    /// Account discriminator (8) + 4 pubkeys (32*4) + bool (1) + u64 (8) + u8 (1)
    pub const LEN: usize = 8 + 32 * 4 + 1 + 8 + 1;
}
