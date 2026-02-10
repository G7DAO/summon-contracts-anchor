use anchor_lang::prelude::*;

/// Tracks reserved amounts for a specific SPL token mint in the treasury.
/// Maps from RewardsState.reservedAmounts (for ERC20/ERC1155).
///
/// PDA seeds: ["reserve", config.key(), mint.key()]
#[account]
pub struct TokenReservation {
    /// The SPL token mint this reservation tracks
    pub mint: Pubkey,
    /// Total reserved amount across all reward tokens
    pub reserved_amount: u64,
    /// PDA bump seed
    pub bump: u8,
}

impl TokenReservation {
    /// 8 (discriminator) + 32 (mint) + 8 (reserved_amount) + 1 (bump)
    pub const LEN: usize = 8 + 32 + 8 + 1;
}

/// Tracks reservation status for a specific NFT mint.
/// Maps from RewardsState.isErc721Reserved.
///
/// PDA seeds: ["nft_reserve", config.key(), nft_mint.key()]
#[account]
pub struct NftReservation {
    /// The NFT mint this reservation tracks
    pub nft_mint: Pubkey,
    /// Whether this NFT is currently reserved for a reward
    pub is_reserved: bool,
    /// PDA bump seed
    pub bump: u8,
}

impl NftReservation {
    /// 8 (discriminator) + 32 (nft_mint) + 1 (is_reserved) + 1 (bump)
    pub const LEN: usize = 8 + 32 + 1 + 1;
}
