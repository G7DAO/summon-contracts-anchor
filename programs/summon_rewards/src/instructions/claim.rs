use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke;
use anchor_spl::token::{self, Token, Transfer};
use anchor_spl::token_2022::Token2022;

use crate::errors::SummonRewardsError;
use crate::events::Claimed;
use crate::state::{
    NftReservation, RewardType, RewardsConfig, RewardTokenState, TokenReservation, TreasuryState,
};

/// Accounts for `claim_reward`.
/// User burns their access token and receives the associated rewards.
/// Maps from Rewards.sol: claimReward / _claimReward / _distributeReward
#[derive(Accounts)]
pub struct ClaimReward<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = !config.is_paused @ SummonRewardsError::ProgramPaused,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
        constraint = !reward_token_state.is_claim_paused @ SummonRewardsError::ClaimRewardPaused,
        constraint = reward_token_state.access_token_mint != Pubkey::default() @ SummonRewardsError::TokenNotExist,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    #[account(
        mut,
        seeds = [b"treasury", config.key().as_ref()],
        bump = treasury_state.bump,
    )]
    /// CHECK: Treasury PDA used as signer for transfers and lamport source for SOL rewards
    pub treasury_state: Account<'info, TreasuryState>,

    /// The Token-2022 access token mint — must match the reward token's mint.
    /// CHECK: Verified via constraint against reward_token_state.access_token_mint.
    #[account(
        mut,
        constraint = access_token_mint.key() == reward_token_state.access_token_mint @ SummonRewardsError::InvalidInput,
    )]
    pub access_token_mint: AccountInfo<'info>,

    /// User's Token-2022 token account holding at least 1 access token.
    /// CHECK: Validated by Token-2022 program during Burn CPI.
    #[account(mut)]
    pub user_token_account: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub token_program_2022: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,

    // remaining_accounts layout for reward distribution:
    // For each reward entry:
    //   Sol:      [] (SOL transfer from treasury PDA lamports)
    //   SplToken: [treasury_token_account, user_token_account, token_reservation]
    //   Nft:      per nft_current_index..(nft_current_index + amount):
    //             [treasury_nft_account, user_nft_account, nft_reservation]
}

/// Accounts for `admin_claim_reward` (manager claims on behalf of user).
/// Maps from Rewards.sol: adminClaimReward
///
/// NOTE: No access-token burn is required for admin claims — the manager is a
/// trusted role and may distribute rewards to arbitrary beneficiaries (e.g.
/// off-chain entitlements, customer-support overrides, etc.).
#[derive(Accounts)]
pub struct AdminClaimReward<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
        constraint = !config.is_paused @ SummonRewardsError::ProgramPaused,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
        constraint = !reward_token_state.is_claim_paused @ SummonRewardsError::ClaimRewardPaused,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    #[account(
        mut,
        seeds = [b"treasury", config.key().as_ref()],
        bump = treasury_state.bump,
    )]
    pub treasury_state: Account<'info, TreasuryState>,

    /// CHECK: The beneficiary receiving the rewards. Validated as non-zero.
    #[account(
        mut,
        constraint = beneficiary.key() != Pubkey::default() @ SummonRewardsError::AddressIsZero,
    )]
    pub beneficiary: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,

    // remaining_accounts: same layout as ClaimReward
}

// ─── Handler Implementations ─────────────────────────────────────────

/// Claim reward — user burns 1 access token and receives rewards.
/// Maps from Rewards.sol: claimReward -> _claimReward -> _distributeReward
///
/// Flow:
/// 1. Burn exactly 1 access token from the user's Token-2022 account
/// 2. Distribute rewards from treasury to user
/// 3. Update reservation amounts
///
/// remaining_accounts layout per reward entry:
///   Sol:      [] - SOL transferred via lamport manipulation from treasury PDA
///   SplToken: [treasury_token_ata, user_token_ata, token_reservation_pda]
///   Nft:      per each NFT to distribute:
///             [treasury_nft_ata, user_nft_ata, nft_reservation_pda]
pub fn claim_reward_handler<'info>(ctx: Context<'_, '_, 'info, 'info, ClaimReward<'info>>) -> Result<()> {
    // ─── Step 1: Burn 1 access token ────────────────────────────────
    let burn_ix = spl_token_2022::instruction::burn(
        &ctx.accounts.token_program_2022.key(),
        &ctx.accounts.user_token_account.key(),
        &ctx.accounts.access_token_mint.key(),
        &ctx.accounts.user.key(), // authority = token holder
        &[],
        1, // burn exactly 1 access token per claim
    )?;

    invoke(
        &burn_ix,
        &[
            ctx.accounts.user_token_account.to_account_info(),
            ctx.accounts.access_token_mint.to_account_info(),
            ctx.accounts.user.to_account_info(),
        ],
    )?;

    // ─── Step 2: Distribute rewards ─────────────────────────────────
    let state = &mut ctx.accounts.reward_token_state;
    let config_key = ctx.accounts.config.key();
    let treasury_bump = ctx.accounts.treasury_state.bump;
    let user_key = ctx.accounts.user.key();

    distribute_rewards(DistributeCtx {
        state,
        config_key,
        treasury_bump,
        recipient: &ctx.accounts.user.to_account_info(),
        treasury_state: &ctx.accounts.treasury_state,
        token_program: &ctx.accounts.token_program,
        remaining_accounts: ctx.remaining_accounts,
        program_id: ctx.program_id,
    })?;

    emit!(Claimed {
        to: user_key,
        token_id: state.token_id,
        amount: 1,
    });

    Ok(())
}

/// Admin claim reward - manager distributes rewards to a beneficiary.
/// Maps from Rewards.sol: adminClaimReward
///
/// No access-token burn — manager is a trusted role.
pub fn admin_claim_reward_handler<'info>(ctx: Context<'_, '_, 'info, 'info, AdminClaimReward<'info>>) -> Result<()> {
    let state = &mut ctx.accounts.reward_token_state;
    let config_key = ctx.accounts.config.key();
    let treasury_bump = ctx.accounts.treasury_state.bump;
    let beneficiary_key = ctx.accounts.beneficiary.key();

    distribute_rewards(DistributeCtx {
        state,
        config_key,
        treasury_bump,
        recipient: &ctx.accounts.beneficiary,
        treasury_state: &ctx.accounts.treasury_state,
        token_program: &ctx.accounts.token_program,
        remaining_accounts: ctx.remaining_accounts,
        program_id: ctx.program_id,
    })?;

    emit!(Claimed {
        to: beneficiary_key,
        token_id: state.token_id,
        amount: 1,
    });

    Ok(())
}

/// Bundled context for `distribute_rewards` to keep arg count ≤ 7.
struct DistributeCtx<'a, 'info> {
    state: &'a mut RewardTokenState,
    config_key: Pubkey,
    treasury_bump: u8,
    recipient: &'a AccountInfo<'info>,
    treasury_state: &'a Account<'info, TreasuryState>,
    token_program: &'a Program<'info, Token>,
    remaining_accounts: &'info [AccountInfo<'info>],
    program_id: &'a Pubkey,
}

/// Internal: distribute all rewards for a single claim.
/// Maps from Rewards.sol: _distributeReward
fn distribute_rewards<'info>(ctx: DistributeCtx<'_, 'info>) -> Result<()> {
    let signer_seeds: &[&[&[u8]]] = &[&[b"treasury", ctx.config_key.as_ref(), &[ctx.treasury_bump]]];

    let mut remaining_idx = 0;

    for reward_idx in 0..ctx.state.rewards.len() {
        let reward = &ctx.state.rewards[reward_idx];

        match reward.reward_type {
            RewardType::Sol => {
                // Transfer SOL (lamports) from treasury PDA to recipient
                let amount = reward.amount;
                let treasury_ai = ctx.treasury_state.to_account_info();

                // Ensure treasury has enough lamports
                require!(
                    treasury_ai.lamports() >= amount,
                    SummonRewardsError::InsufficientBalance
                );

                // Direct lamport transfer: debit treasury, credit recipient
                **treasury_ai.try_borrow_mut_lamports()? -= amount;
                **ctx.recipient.try_borrow_mut_lamports()? += amount;
            }
            RewardType::SplToken => {
                // Transfer SPL tokens from treasury to recipient
                require!(
                    remaining_idx + 3 <= ctx.remaining_accounts.len(),
                    SummonRewardsError::InvalidInput
                );

                let treasury_token_ai = &ctx.remaining_accounts[remaining_idx];
                let user_token_ai = &ctx.remaining_accounts[remaining_idx + 1];
                let reservation_ai = &ctx.remaining_accounts[remaining_idx + 2];
                remaining_idx += 3;

                // Transfer tokens
                token::transfer(
                    CpiContext::new_with_signer(
                        ctx.token_program.to_account_info(),
                        Transfer {
                            from: treasury_token_ai.clone(),
                            to: user_token_ai.clone(),
                            authority: ctx.treasury_state.to_account_info(),
                        },
                        signer_seeds,
                    ),
                    reward.amount,
                )?;

                // Decrease reservation
                let mut reservation: Account<TokenReservation> =
                    Account::try_from(reservation_ai)?;

                let (expected_pda, _) = Pubkey::find_program_address(
                    &[
                        b"reserve",
                        ctx.config_key.as_ref(),
                        reward.token_mint.unwrap().as_ref(),
                    ],
                    ctx.program_id,
                );
                require!(
                    reservation_ai.key() == expected_pda,
                    SummonRewardsError::InvalidInput
                );

                reservation.reserved_amount = reservation
                    .reserved_amount
                    .checked_sub(reward.amount)
                    .ok_or(SummonRewardsError::ArithmeticOverflow)?;
                reservation.exit(ctx.program_id)?;
            }
            RewardType::Nft => {
                // Transfer NFTs from treasury to recipient
                let current_index = reward.nft_current_index as usize;
                let nft_count = reward.amount as usize;

                for j in 0..nft_count {
                    let nft_idx = current_index + j;
                    require!(
                        nft_idx < reward.nft_mints.len(),
                        SummonRewardsError::InsufficientBalance
                    );

                    require!(
                        remaining_idx + 3 <= ctx.remaining_accounts.len(),
                        SummonRewardsError::InvalidInput
                    );

                    let treasury_nft_ai = &ctx.remaining_accounts[remaining_idx];
                    let user_nft_ai = &ctx.remaining_accounts[remaining_idx + 1];
                    let nft_reservation_ai = &ctx.remaining_accounts[remaining_idx + 2];
                    remaining_idx += 3;

                    // Transfer NFT (amount = 1)
                    token::transfer(
                        CpiContext::new_with_signer(
                            ctx.token_program.to_account_info(),
                            Transfer {
                                from: treasury_nft_ai.clone(),
                                to: user_nft_ai.clone(),
                                authority: ctx.treasury_state.to_account_info(),
                            },
                            signer_seeds,
                        ),
                        1,
                    )?;

                    // Release NFT reservation
                    let nft_mint = reward.nft_mints[nft_idx];
                    let mut nft_reservation: Account<NftReservation> =
                        Account::try_from(nft_reservation_ai)?;

                    let (expected_pda, _) = Pubkey::find_program_address(
                        &[
                            b"nft_reserve",
                            ctx.config_key.as_ref(),
                            nft_mint.as_ref(),
                        ],
                        ctx.program_id,
                    );
                    require!(
                        nft_reservation_ai.key() == expected_pda,
                        SummonRewardsError::InvalidInput
                    );

                    nft_reservation.is_reserved = false;
                    nft_reservation.exit(ctx.program_id)?;
                }

                // Increment the NFT current index for this reward
                ctx.state.rewards[reward_idx].nft_current_index = (current_index + nft_count) as u64;
            }
        }
    }

    Ok(())
}
