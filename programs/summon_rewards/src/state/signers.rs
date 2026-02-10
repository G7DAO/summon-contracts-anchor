use anchor_lang::prelude::*;

/// Whitelist signers account for Ed25519 signature verification.
/// Maps from ERCWhitelistSignatureUpgradeable.whitelistSigners.
///
/// PDA seeds: ["signers", config.key()]
#[account]
pub struct WhitelistSigners {
    /// List of authorized Ed25519 signer public keys
    pub signers: Vec<Pubkey>,
    /// PDA bump seed
    pub bump: u8,
}

impl WhitelistSigners {
    /// Calculate space for a given max number of signers.
    /// Typically limited to ~10 signers.
    pub fn space(max_signers: usize) -> usize {
        8 // discriminator
        + 4 // vec length prefix
        + max_signers * 32 // Pubkey per signer
        + 1 // bump
    }
}
