use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::errors::SummonRewardsError;
use crate::events::{TokenRemovedFromWhitelist, TokenWhitelisted, TreasuryDeposit};
use crate::state::{
    NftReservation, RewardType, RewardsConfig, TokenReservation, TokenWhitelist, TreasuryState,
    WhitelistEntry,
};

/// Accounts for `whitelist_token`.
#[derive(Accounts)]
pub struct WhitelistToken<'info> {
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"whitelist", config.key().as_ref()],
        bump = token_whitelist.bump,
    )]
    pub token_whitelist: Account<'info, TokenWhitelist>,
}

/// Accounts for `remove_token_from_whitelist`.
#[derive(Accounts)]
#[instruction(mint: Pubkey)]
pub struct RemoveTokenFromWhitelist<'info> {
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"whitelist", config.key().as_ref()],
        bump = token_whitelist.bump,
    )]
    pub token_whitelist: Account<'info, TokenWhitelist>,
}

/// Accounts for `deposit_to_treasury` (SPL token deposit).
#[derive(Accounts)]
pub struct DepositToTreasury<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        seeds = [b"whitelist", config.key().as_ref()],
        bump = token_whitelist.bump,
    )]
    pub token_whitelist: Account<'info, TokenWhitelist>,

    #[account(
        seeds = [b"treasury", config.key().as_ref()],
        bump = treasury_state.bump,
    )]
    pub treasury_state: Account<'info, TreasuryState>,

    /// The SPL token mint being deposited.
    pub mint: Account<'info, Mint>,

    /// Depositor's token account.
    #[account(
        mut,
        constraint = depositor_token_account.mint == mint.key(),
        constraint = depositor_token_account.owner == depositor.key(),
    )]
    pub depositor_token_account: Account<'info, TokenAccount>,

    /// Treasury's token account (ATA owned by treasury PDA).
    #[account(
        mut,
        constraint = treasury_token_account.mint == mint.key(),
        constraint = treasury_token_account.owner == treasury_state.key(),
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

/// Accounts for `withdraw_unreserved_treasury`.
#[derive(Accounts)]
pub struct WithdrawUnreservedTreasury<'info> {
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        seeds = [b"treasury", config.key().as_ref()],
        bump = treasury_state.bump,
    )]
    pub treasury_state: Account<'info, TreasuryState>,

    /// The SPL token mint.
    pub mint: Account<'info, Mint>,

    /// Treasury's token account.
    #[account(
        mut,
        constraint = treasury_token_account.mint == mint.key(),
        constraint = treasury_token_account.owner == treasury_state.key(),
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    /// Destination token account.
    #[account(
        mut,
        constraint = destination_token_account.mint == mint.key(),
    )]
    pub destination_token_account: Account<'info, TokenAccount>,

    /// Token reservation PDA for this mint.
    #[account(
        seeds = [b"reserve", config.key().as_ref(), mint.key().as_ref()],
        bump = token_reservation.bump,
    )]
    pub token_reservation: Account<'info, TokenReservation>,

    pub token_program: Program<'info, Token>,
}

/// Accounts for `deposit_nft_to_treasury`.
#[derive(Accounts)]
pub struct DepositNftToTreasury<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        seeds = [b"whitelist", config.key().as_ref()],
        bump = token_whitelist.bump,
    )]
    pub token_whitelist: Account<'info, TokenWhitelist>,

    #[account(
        seeds = [b"treasury", config.key().as_ref()],
        bump = treasury_state.bump,
    )]
    pub treasury_state: Account<'info, TreasuryState>,

    /// The NFT mint (supply=1, decimals=0).
    #[account(
        constraint = nft_mint.decimals == 0 @ SummonRewardsError::InvalidInput,
    )]
    pub nft_mint: Account<'info, Mint>,

    /// Depositor's NFT token account.
    #[account(
        mut,
        constraint = depositor_token_account.mint == nft_mint.key(),
        constraint = depositor_token_account.owner == depositor.key(),
        constraint = depositor_token_account.amount == 1 @ SummonRewardsError::InsufficientBalance,
    )]
    pub depositor_token_account: Account<'info, TokenAccount>,

    /// Treasury's NFT token account.
    #[account(
        mut,
        constraint = treasury_token_account.mint == nft_mint.key(),
        constraint = treasury_token_account.owner == treasury_state.key(),
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

/// Accounts for `withdraw_unreserved_nft`.
#[derive(Accounts)]
pub struct WithdrawUnreservedNft<'info> {
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        seeds = [b"treasury", config.key().as_ref()],
        bump = treasury_state.bump,
    )]
    pub treasury_state: Account<'info, TreasuryState>,

    /// The NFT mint.
    #[account(
        constraint = nft_mint.decimals == 0 @ SummonRewardsError::InvalidInput,
    )]
    pub nft_mint: Account<'info, Mint>,

    /// NFT reservation PDA - check that NFT is not reserved.
    #[account(
        seeds = [b"nft_reserve", config.key().as_ref(), nft_mint.key().as_ref()],
        bump = nft_reservation.bump,
        constraint = !nft_reservation.is_reserved @ SummonRewardsError::NftAlreadyReserved,
    )]
    pub nft_reservation: Account<'info, NftReservation>,

    /// Treasury's NFT token account.
    #[account(
        mut,
        constraint = treasury_token_account.mint == nft_mint.key(),
        constraint = treasury_token_account.owner == treasury_state.key(),
        constraint = treasury_token_account.amount == 1 @ SummonRewardsError::InsufficientBalance,
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    /// Destination NFT token account.
    #[account(
        mut,
        constraint = destination_token_account.mint == nft_mint.key(),
    )]
    pub destination_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

// ─── Handler Implementations ─────────────────────────────────────────

/// Whitelist a token mint for treasury deposits.
/// Maps from Treasury.sol: whitelistToken -> RewardsState.whitelistToken
pub fn whitelist_token_handler(
    ctx: Context<WhitelistToken>,
    mint: Pubkey,
    reward_type: RewardType,
) -> Result<()> {
    require!(
        mint != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );

    let whitelist = &mut ctx.accounts.token_whitelist;

    // Check not already whitelisted
    let already_exists = whitelist
        .tokens
        .iter()
        .any(|entry| entry.mint == mint && entry.is_active);
    require!(
        !already_exists,
        SummonRewardsError::TokenAlreadyWhitelisted
    );

    whitelist.tokens.push(WhitelistEntry {
        mint,
        reward_type: reward_type.clone(),
        is_active: true,
    });

    emit!(TokenWhitelisted { mint, reward_type });

    Ok(())
}

/// Remove a token from the whitelist.
/// Maps from Treasury.sol: removeTokenFromWhitelist
///
/// Requires that the token has no active reservations before removal.
/// The caller must pass the TokenReservation PDA via remaining_accounts
/// for SPL tokens so we can verify reserved_amount == 0.
pub fn remove_token_from_whitelist_handler<'info>(
    ctx: Context<'_, '_, 'info, 'info, RemoveTokenFromWhitelist<'info>>,
    mint: Pubkey,
) -> Result<()> {
    // First, find the position and reward type (immutable borrow)
    let pos = ctx.accounts.token_whitelist
        .tokens
        .iter()
        .position(|entry| entry.mint == mint && entry.is_active)
        .ok_or(SummonRewardsError::TokenNotWhitelisted)?;

    let reward_type = ctx.accounts.token_whitelist.tokens[pos].reward_type.clone();

    // For SPL tokens, verify no active reservations exist
    if reward_type == RewardType::SplToken {
        let remaining = ctx.remaining_accounts;
        if !remaining.is_empty() {
            let config_key = ctx.accounts.config.key();
            let reservation_ai = &remaining[0];

            // Verify PDA seeds
            let (expected_pda, _) = Pubkey::find_program_address(
                &[b"reserve", config_key.as_ref(), mint.as_ref()],
                ctx.program_id,
            );
            if reservation_ai.key() == expected_pda {
                let reservation: Account<TokenReservation> =
                    Account::try_from(reservation_ai)?;
                require!(
                    reservation.reserved_amount == 0,
                    SummonRewardsError::TokenHasReserves
                );
            }
        }
    }

    // Now take mutable borrow for removal
    ctx.accounts.token_whitelist.tokens.swap_remove(pos);

    emit!(TokenRemovedFromWhitelist { mint });

    Ok(())
}

/// Deposit SPL tokens to the treasury.
/// Maps from Treasury.sol: depositToTreasury
pub fn deposit_to_treasury_handler(ctx: Context<DepositToTreasury>, amount: u64) -> Result<()> {
    require!(amount > 0, SummonRewardsError::InvalidAmount);

    // Verify token is whitelisted as SplToken type
    let mint_key = ctx.accounts.mint.key();
    let is_whitelisted = ctx
        .accounts
        .token_whitelist
        .tokens
        .iter()
        .any(|entry| entry.mint == mint_key && entry.is_active && entry.reward_type == RewardType::SplToken);
    require!(is_whitelisted, SummonRewardsError::TokenNotWhitelisted);

    // Transfer tokens from depositor to treasury
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.depositor_token_account.to_account_info(),
                to: ctx.accounts.treasury_token_account.to_account_info(),
                authority: ctx.accounts.depositor.to_account_info(),
            },
        ),
        amount,
    )?;

    emit!(TreasuryDeposit {
        mint: mint_key,
        amount,
    });

    Ok(())
}

/// Withdraw unreserved SPL tokens from the treasury.
/// Maps from Treasury.sol: withdrawUnreservedTreasury
/// Transfers (balance - reserved) to destination.
pub fn withdraw_unreserved_treasury_handler(
    ctx: Context<WithdrawUnreservedTreasury>,
) -> Result<()> {
    let balance = ctx.accounts.treasury_token_account.amount;
    let reserved = ctx.accounts.token_reservation.reserved_amount;

    require!(balance > reserved, SummonRewardsError::InsufficientBalance);

    let withdraw_amount = balance
        .checked_sub(reserved)
        .ok_or(SummonRewardsError::ArithmeticOverflow)?;

    // Sign the CPI with the treasury PDA seeds
    let config_key = ctx.accounts.config.key();
    let treasury_bump = ctx.accounts.treasury_state.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[b"treasury", config_key.as_ref(), &[treasury_bump]]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.treasury_token_account.to_account_info(),
                to: ctx.accounts.destination_token_account.to_account_info(),
                authority: ctx.accounts.treasury_state.to_account_info(),
            },
            signer_seeds,
        ),
        withdraw_amount,
    )?;

    Ok(())
}

/// Deposit an NFT (supply=1 token) to the treasury.
/// Maps from Treasury.sol: implicit NFT deposit via ERC721 safeTransferFrom
pub fn deposit_nft_to_treasury_handler(ctx: Context<DepositNftToTreasury>) -> Result<()> {
    // Verify NFT is whitelisted as Nft type
    let nft_mint_key = ctx.accounts.nft_mint.key();
    let is_whitelisted = ctx
        .accounts
        .token_whitelist
        .tokens
        .iter()
        .any(|entry| entry.mint == nft_mint_key && entry.is_active && entry.reward_type == RewardType::Nft);
    require!(is_whitelisted, SummonRewardsError::TokenNotWhitelisted);

    // Transfer NFT from depositor to treasury
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.depositor_token_account.to_account_info(),
                to: ctx.accounts.treasury_token_account.to_account_info(),
                authority: ctx.accounts.depositor.to_account_info(),
            },
        ),
        1,
    )?;

    emit!(TreasuryDeposit {
        mint: nft_mint_key,
        amount: 1,
    });

    Ok(())
}

/// Withdraw an unreserved NFT from the treasury.
/// Maps from Treasury.sol: withdrawERC721UnreservedTreasury
pub fn withdraw_unreserved_nft_handler(ctx: Context<WithdrawUnreservedNft>) -> Result<()> {
    // NftReservation constraint already validates !is_reserved

    let config_key = ctx.accounts.config.key();
    let treasury_bump = ctx.accounts.treasury_state.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[b"treasury", config_key.as_ref(), &[treasury_bump]]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.treasury_token_account.to_account_info(),
                to: ctx.accounts.destination_token_account.to_account_info(),
                authority: ctx.accounts.treasury_state.to_account_info(),
            },
            signer_seeds,
        ),
        1,
    )?;

    Ok(())
}
