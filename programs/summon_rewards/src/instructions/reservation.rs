use anchor_lang::prelude::*;

use crate::errors::SummonRewardsError;
use crate::state::{NftReservation, RewardsConfig, TokenReservation};

/// Accounts for `init_token_reservation`.
/// Initializes a TokenReservation PDA for tracking reserved SPL token amounts.
/// Must be called before `create_reward_token` for any SPL reward mint.
#[derive(Accounts)]
#[instruction(mint: Pubkey)]
pub struct InitTokenReservation<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        init,
        payer = manager,
        space = TokenReservation::LEN,
        seeds = [b"reserve", config.key().as_ref(), mint.as_ref()],
        bump,
    )]
    pub token_reservation: Account<'info, TokenReservation>,

    pub system_program: Program<'info, System>,
}

/// Accounts for `init_nft_reservation`.
/// Initializes an NftReservation PDA for tracking a specific NFT's reservation status.
/// Must be called before `create_reward_token` for any NFT reward mint.
#[derive(Accounts)]
#[instruction(nft_mint: Pubkey)]
pub struct InitNftReservation<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        init,
        payer = manager,
        space = NftReservation::LEN,
        seeds = [b"nft_reserve", config.key().as_ref(), nft_mint.as_ref()],
        bump,
    )]
    pub nft_reservation: Account<'info, NftReservation>,

    pub system_program: Program<'info, System>,
}

/// Initialize a TokenReservation PDA for an SPL token mint.
pub fn init_token_reservation_handler(
    ctx: Context<InitTokenReservation>,
    mint: Pubkey,
) -> Result<()> {
    require!(
        mint != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );

    let reservation = &mut ctx.accounts.token_reservation;
    reservation.mint = mint;
    reservation.reserved_amount = 0;
    reservation.bump = ctx.bumps.token_reservation;

    Ok(())
}

/// Initialize an NftReservation PDA for a specific NFT mint.
pub fn init_nft_reservation_handler(
    ctx: Context<InitNftReservation>,
    nft_mint: Pubkey,
) -> Result<()> {
    require!(
        nft_mint != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );

    let reservation = &mut ctx.accounts.nft_reservation;
    reservation.nft_mint = nft_mint;
    reservation.is_reserved = false;
    reservation.bump = ctx.bumps.nft_reservation;

    Ok(())
}
