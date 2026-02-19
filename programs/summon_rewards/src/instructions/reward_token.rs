use anchor_lang::prelude::*;
use anchor_spl::token::{TokenAccount};

use crate::errors::SummonRewardsError;
use crate::events::{
    ClaimRewardPausedUpdated, RewardSupplyChanged, RewardTokenCreated, TokenMintPausedUpdated,
    TokenUriChanged,
};
use crate::state::{
    NftReservation, RewardEntry, RewardType, RewardsConfig, RewardTokenState, TokenReservation,
    TreasuryState,
};

/// Accounts for `create_reward_token`.
#[derive(Accounts)]
#[instruction(token_id: u64, token_uri: String, max_supply: u64, rewards: Vec<RewardEntry>)]
pub struct CreateRewardToken<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        init,
        payer = manager,
        space = RewardTokenState::space(rewards.len(), 100),
        seeds = [b"reward_token", config.key().as_ref(), &token_id.to_le_bytes()],
        bump,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    #[account(
        seeds = [b"treasury", config.key().as_ref()],
        bump = treasury_state.bump,
    )]
    pub treasury_state: Account<'info, TreasuryState>,

    pub system_program: Program<'info, System>,
    // remaining_accounts layout for reservation validation:
    // For each SPL reward: [treasury_token_account, token_reservation_pda]
    // For each NFT reward: [nft_reservation_pda] (per NFT mint in nft_mints)
}

/// Accounts for `update_token_mint_paused` / `update_claim_paused`.
#[derive(Accounts)]
pub struct UpdateTokenPaused<'info> {
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,
}

/// Accounts for `increase_reward_supply`.
#[derive(Accounts)]
pub struct IncreaseRewardSupply<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    #[account(
        seeds = [b"treasury", config.key().as_ref()],
        bump = treasury_state.bump,
    )]
    pub treasury_state: Account<'info, TreasuryState>,
    // remaining_accounts: same layout as create_reward_token for reservation updates
}

/// Accounts for `update_token_uri`.
#[derive(Accounts)]
pub struct UpdateTokenUri<'info> {
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,
}

// ─── Handler Implementations ─────────────────────────────────────────

/// Create a new reward token with associated rewards.
/// Maps from Rewards.sol: _createTokenAndDepositRewards
///
/// Validates inputs, checks treasury balances, reserves amounts for SPL and NFT rewards.
/// SOL rewards are validated by requiring manager to deposit lamports to treasury PDA.
///
/// remaining_accounts expected layout:
///   For each SPL reward entry: [treasury_token_account (read), token_reservation (write)]
///   For each NFT reward entry: [nft_reservation (write)] per NFT mint
pub fn create_reward_token_handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, CreateRewardToken<'info>>,
    token_id: u64,
    token_uri: String,
    max_supply: u64,
    rewards: Vec<RewardEntry>,
) -> Result<()> {
    // Validate basic inputs (maps from _validateTokenInputs)
    require!(max_supply > 0, SummonRewardsError::InvalidAmount);
    require!(token_id > 0, SummonRewardsError::InvalidInput);
    require!(!token_uri.is_empty(), SummonRewardsError::InvalidInput);
    require!(token_uri.len() <= 200, SummonRewardsError::InvalidInput);
    require!(!rewards.is_empty(), SummonRewardsError::InvalidInput);

    // Validate each reward entry
    for reward in rewards.iter() {
        match reward.reward_type {
            RewardType::Sol => {
                require!(reward.amount > 0, SummonRewardsError::InvalidAmount);
            }
            RewardType::SplToken => {
                require!(
                    reward.token_mint.is_some(),
                    SummonRewardsError::AddressIsZero
                );
                require!(reward.amount > 0, SummonRewardsError::InvalidAmount);
            }
            RewardType::Nft => {
                require!(
                    reward.token_mint.is_some(),
                    SummonRewardsError::AddressIsZero
                );
                // NFT rewards: nft_mints length must equal amount * max_supply
                let expected_nfts = (reward.amount as u128)
                    .checked_mul(max_supply as u128)
                    .ok_or(SummonRewardsError::ArithmeticOverflow)?;
                require!(
                    reward.nft_mints.len() as u128 == expected_nfts,
                    SummonRewardsError::InvalidInput
                );
            }
        }
    }

    // Process remaining_accounts for reservation validation
    let remaining = &ctx.remaining_accounts;
    let mut remaining_idx = 0;
    let config_key = ctx.accounts.config.key();
    let treasury_key = ctx.accounts.treasury_state.key();

    for reward in rewards.iter() {
        match reward.reward_type {
            RewardType::Sol => {
                // SOL rewards: validate treasury PDA has enough lamports
                // Total SOL needed = reward.amount * max_supply
                // This is validated at claim time; manager must pre-fund treasury
            }
            RewardType::SplToken => {
                // Need: treasury_token_account, token_reservation PDA
                require!(
                    remaining_idx + 2 <= remaining.len(),
                    SummonRewardsError::InvalidInput
                );

                let treasury_token_ai = &remaining[remaining_idx];
                let reservation_ai = &remaining[remaining_idx + 1];
                remaining_idx += 2;

                // Deserialize treasury token account to check balance
                let treasury_token: Account<TokenAccount> =
                    Account::try_from(treasury_token_ai)?;
                require!(
                    treasury_token.owner == treasury_key,
                    SummonRewardsError::InvalidInput
                );
                require!(
                    treasury_token.mint == reward.token_mint.unwrap(),
                    SummonRewardsError::InvalidInput
                );

                // Deserialize token reservation
                let mut reservation: Account<TokenReservation> =
                    Account::try_from(reservation_ai)?;

                // Verify PDA seeds
                let (expected_pda, _) = Pubkey::find_program_address(
                    &[
                        b"reserve",
                        config_key.as_ref(),
                        reward.token_mint.unwrap().as_ref(),
                    ],
                    ctx.program_id,
                );
                require!(
                    reservation_ai.key() == expected_pda,
                    SummonRewardsError::InvalidInput
                );

                // Check treasury has enough: balance >= reserved + totalAmount
                let total_amount = reward
                    .amount
                    .checked_mul(max_supply)
                    .ok_or(SummonRewardsError::ArithmeticOverflow)?;
                let new_reserved = reservation
                    .reserved_amount
                    .checked_add(total_amount)
                    .ok_or(SummonRewardsError::ArithmeticOverflow)?;
                require!(
                    treasury_token.amount >= new_reserved,
                    SummonRewardsError::InsufficientTreasuryBalance
                );

                // Update reservation
                reservation.reserved_amount = new_reserved;
                reservation.exit(ctx.program_id)?;
            }
            RewardType::Nft => {
                // For each NFT mint in nft_mints, need an NftReservation PDA
                for nft_mint in reward.nft_mints.iter() {
                    require!(
                        remaining_idx < remaining.len(),
                        SummonRewardsError::InvalidInput
                    );

                    let nft_reservation_ai = &remaining[remaining_idx];
                    remaining_idx += 1;

                    let mut nft_reservation: Account<NftReservation> =
                        Account::try_from(nft_reservation_ai)?;

                    // Verify PDA seeds
                    let (expected_pda, _) = Pubkey::find_program_address(
                        &[
                            b"nft_reserve",
                            config_key.as_ref(),
                            nft_mint.as_ref(),
                        ],
                        ctx.program_id,
                    );
                    require!(
                        nft_reservation_ai.key() == expected_pda,
                        SummonRewardsError::InvalidInput
                    );

                    // Check not already reserved
                    require!(
                        !nft_reservation.is_reserved,
                        SummonRewardsError::NftAlreadyReserved
                    );

                    // Reserve the NFT
                    nft_reservation.is_reserved = true;
                    nft_reservation.exit(ctx.program_id)?;
                }
            }
        }
    }

    // Initialize the reward token state
    let state = &mut ctx.accounts.reward_token_state;
    state.token_id = token_id;
    state.token_uri = token_uri.clone();
    state.max_supply = max_supply;
    state.current_supply = 0;
    state.is_mint_paused = false;
    state.is_claim_paused = false;
    state.access_token_mint = Pubkey::default(); // Set when create_access_token_mint is called
    state.rewards = rewards;
    state.bump = ctx.bumps.reward_token_state;

    // Increment global token count
    let config = &mut ctx.accounts.config;
    config.reward_token_count = config
        .reward_token_count
        .checked_add(1)
        .ok_or(SummonRewardsError::ArithmeticOverflow)?;

    emit!(RewardTokenCreated {
        token_id,
        max_supply,
        token_uri,
    });

    Ok(())
}

/// Update the mint paused status for a reward token.
/// Maps from Rewards.sol: updateTokenMintPaused
pub fn update_token_mint_paused_handler(
    ctx: Context<UpdateTokenPaused>,
    is_paused: bool,
) -> Result<()> {
    let state = &mut ctx.accounts.reward_token_state;
    state.is_mint_paused = is_paused;

    emit!(TokenMintPausedUpdated {
        token_id: state.token_id,
        is_paused,
    });

    Ok(())
}

/// Update the claim paused status for a reward token.
/// Maps from Rewards.sol: updateClaimRewardPaused
pub fn update_claim_paused_handler(
    ctx: Context<UpdateTokenPaused>,
    is_paused: bool,
) -> Result<()> {
    let state = &mut ctx.accounts.reward_token_state;
    state.is_claim_paused = is_paused;

    emit!(ClaimRewardPausedUpdated {
        token_id: state.token_id,
        is_paused,
    });

    Ok(())
}

/// Increase the max supply of a reward token.
/// Maps from Rewards.sol: increaseRewardSupply
///
/// Validates treasury has enough balance for the additional supply
/// and increases reservation amounts accordingly.
///
/// remaining_accounts layout (same as create):
///   For each SPL reward: [treasury_token_account (read), token_reservation (write)]
pub fn increase_reward_supply_handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, IncreaseRewardSupply<'info>>,
    additional_supply: u64,
) -> Result<()> {
    require!(additional_supply > 0, SummonRewardsError::InvalidAmount);

    let state = &mut ctx.accounts.reward_token_state;

    // Block supply increase for reward tokens that include NFT rewards.
    // NFTs require individual mint addresses to be provided upfront; increasing
    // max_supply without new NFT mints would create an inconsistent state where
    // claims could fail for the extra supply.
    let has_nft_rewards = state
        .rewards
        .iter()
        .any(|r| matches!(r.reward_type, RewardType::Nft));
    require!(
        !has_nft_rewards,
        SummonRewardsError::NftSupplyIncreaseNotSupported
    );

    let old_supply = state.max_supply;
    let new_supply = old_supply
        .checked_add(additional_supply)
        .ok_or(SummonRewardsError::ArithmeticOverflow)?;

    // Process remaining_accounts for SPL reservation increases
    let remaining = &ctx.remaining_accounts;
    let mut remaining_idx = 0;
    let config_key = ctx.accounts.config.key();
    let treasury_key = ctx.accounts.treasury_state.key();

    for reward in state.rewards.iter() {
        match reward.reward_type {
            RewardType::Sol => {
                // SOL is validated at claim time
            }
            RewardType::SplToken => {
                require!(
                    remaining_idx + 2 <= remaining.len(),
                    SummonRewardsError::InvalidInput
                );

                let treasury_token_ai = &remaining[remaining_idx];
                let reservation_ai = &remaining[remaining_idx + 1];
                remaining_idx += 2;

                let treasury_token: Account<TokenAccount> =
                    Account::try_from(treasury_token_ai)?;
                require!(
                    treasury_token.owner == treasury_key,
                    SummonRewardsError::InvalidInput
                );

                let mut reservation: Account<TokenReservation> =
                    Account::try_from(reservation_ai)?;

                let (expected_pda, _) = Pubkey::find_program_address(
                    &[
                        b"reserve",
                        config_key.as_ref(),
                        reward.token_mint.unwrap().as_ref(),
                    ],
                    ctx.program_id,
                );
                require!(
                    reservation_ai.key() == expected_pda,
                    SummonRewardsError::InvalidInput
                );

                let additional_amount = reward
                    .amount
                    .checked_mul(additional_supply)
                    .ok_or(SummonRewardsError::ArithmeticOverflow)?;
                let new_reserved = reservation
                    .reserved_amount
                    .checked_add(additional_amount)
                    .ok_or(SummonRewardsError::ArithmeticOverflow)?;
                require!(
                    treasury_token.amount >= new_reserved,
                    SummonRewardsError::InsufficientTreasuryBalance
                );

                reservation.reserved_amount = new_reserved;
                reservation.exit(ctx.program_id)?;
            }
            RewardType::Nft => {
                // NFT supply increase requires providing new NFT mints
                // which is more complex - for now, NFT supply increase
                // would need a separate instruction to add NFT mints
                // to the reward entry. This matches Solidity behavior where
                // increaseRewardSupply only handles ERC20/ERC1155 reservations.
            }
        }
    }

    state.max_supply = new_supply;

    emit!(RewardSupplyChanged {
        token_id: state.token_id,
        old_supply,
        new_supply,
    });

    Ok(())
}

/// Update the token URI for a reward token.
/// Maps from Rewards.sol: updateTokenUri
pub fn update_token_uri_handler(ctx: Context<UpdateTokenUri>, new_uri: String) -> Result<()> {
    require!(!new_uri.is_empty(), SummonRewardsError::InvalidInput);
    require!(new_uri.len() <= 200, SummonRewardsError::InvalidInput);

    let state = &mut ctx.accounts.reward_token_state;
    state.token_uri = new_uri.clone();

    emit!(TokenUriChanged {
        token_id: state.token_id,
        new_uri,
    });

    Ok(())
}
