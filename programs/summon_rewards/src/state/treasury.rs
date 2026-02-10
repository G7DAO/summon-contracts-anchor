use anchor_lang::prelude::*;

/// Treasury state PDA. The treasury PDA itself serves as the authority
/// for all treasury-owned token accounts (ATAs).
/// For SOL, the PDA holds lamports directly.
///
/// Maps from Treasury.sol - but simplified since on Solana the PDA
/// itself acts as the vault via its owned ATAs.
///
/// PDA seeds: ["treasury", config.key()]
#[account]
pub struct TreasuryState {
    /// PDA bump seed
    pub bump: u8,
}

impl TreasuryState {
    pub const LEN: usize = 8 + 1; // discriminator + bump
}
