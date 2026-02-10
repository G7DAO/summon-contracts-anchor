use anchor_lang::prelude::*;

/// Per-user nonce tracking for signature replay protection.
/// Maps from RewardsState.userNonces[user][nonce].
///
/// On Solana we use a PDA-per-nonce approach: if the PDA exists
/// and used == true, the nonce has been consumed.
///
/// PDA seeds: ["nonce", config.key(), user.key(), nonce.to_le_bytes()]
#[account]
pub struct UserNonce {
    /// Whether this nonce has been used
    pub used: bool,
    /// PDA bump seed
    pub bump: u8,
}

impl UserNonce {
    /// 8 (discriminator) + 1 (used) + 1 (bump)
    pub const LEN: usize = 8 + 1 + 1;
}
