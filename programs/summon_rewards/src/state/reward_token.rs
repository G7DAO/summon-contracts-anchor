use anchor_lang::prelude::*;

/// Reward type enum - simplified from Solidity's 4-type enum.
/// On Solana, ERC20 and ERC1155 both map to SPL tokens.
///
/// Maps from LibItems.RewardType:
///   ETHER  -> Sol
///   ERC20  -> SplToken
///   ERC721 -> Nft
///   ERC1155 -> SplToken (collapsed, same as ERC20 on Solana)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum RewardType {
    /// Native SOL (was ETHER in Solidity)
    Sol,
    /// Fungible SPL token (covers both ERC20 and ERC1155)
    SplToken,
    /// NFT with supply=1, decimals=0 (was ERC721 in Solidity)
    Nft,
}

/// A single reward entry within a RewardToken definition.
/// Maps from LibItems.Reward struct.
///
/// For Sol: amount is in lamports, token_mint is None
/// For SplToken: amount is in token base units, token_mint is the SPL mint
/// For Nft: amount is NFTs per claim, nft_mints lists all NFT mints to distribute
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct RewardEntry {
    /// Type of reward
    pub reward_type: RewardType,
    /// Amount per claim (lamports for SOL, base units for SPL)
    pub amount: u64,
    /// SPL token mint address (None for SOL rewards)
    pub token_mint: Option<Pubkey>,
    /// For NFT rewards: list of NFT mint addresses to distribute
    pub nft_mints: Vec<Pubkey>,
    /// For NFT rewards: tracks which NFT to distribute next
    /// Maps from erc721RewardCurrentIndex[rewardTokenId][rewardIndex]
    pub nft_current_index: u64,
}

/// Per-reward-token state account.
/// Maps from the combination of:
///   - RewardsState.tokenRewards[tokenId]
///   - RewardsState.currentRewardSupply[tokenId]
///   - RewardsState.isTokenMintPaused[tokenId]
///   - RewardsState.isClaimRewardPaused[tokenId]
///
/// PDA seeds: ["reward_token", config.key(), token_id.to_le_bytes()]
#[account]
pub struct RewardTokenState {
    /// Unique token ID
    pub token_id: u64,
    /// Metadata URI (max 200 chars)
    pub token_uri: String,
    /// Maximum supply (0 means unlimited in Solidity, but we require > 0)
    pub max_supply: u64,
    /// Current minted supply
    pub current_supply: u64,
    /// Whether minting is paused for this token
    pub is_mint_paused: bool,
    /// Whether claiming is paused for this token
    pub is_claim_paused: bool,
    /// Token-2022 mint for the access/reward token (soulbound)
    /// Set to Pubkey::default() until create_access_token_mint is called
    pub access_token_mint: Pubkey,
    /// The rewards attached to this token
    pub rewards: Vec<RewardEntry>,
    /// PDA bump seed
    pub bump: u8,
}

impl RewardTokenState {
    /// Calculate the space needed for this account.
    /// This is dynamic due to Vec fields, so we compute a max size.
    /// Base: 8 (discriminator) + 8 (token_id) + 4+200 (string) + 8 (max_supply)
    ///       + 8 (current_supply) + 1 (is_mint_paused) + 1 (is_claim_paused)
    ///       + 32 (access_token_mint) + 4 (vec len) + rewards_size + 1 (bump)
    pub fn space(num_rewards: usize, max_nfts_per_reward: usize) -> usize {
        8 // discriminator
        + 8 // token_id
        + 4 + 200 // token_uri (string prefix + max chars)
        + 8 // max_supply
        + 8 // current_supply
        + 1 // is_mint_paused
        + 1 // is_claim_paused
        + 32 // access_token_mint
        + 4 // rewards vec length prefix
        + num_rewards * Self::reward_entry_size(max_nfts_per_reward)
        + 1 // bump
    }

    fn reward_entry_size(max_nfts: usize) -> usize {
        1 // reward_type enum
        + 8 // amount
        + 1 + 32 // Option<Pubkey> (1 byte tag + 32 bytes pubkey)
        + 4 + max_nfts * 32 // nft_mints vec
        + 8 // nft_current_index
    }
}
