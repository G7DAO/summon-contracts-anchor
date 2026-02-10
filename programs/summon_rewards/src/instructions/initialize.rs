use anchor_lang::prelude::*;

use crate::errors::SummonRewardsError;
use crate::events::ProgramInitialized;
use crate::state::{RewardsConfig, TokenWhitelist, TreasuryState, WhitelistSigners};

/// Accounts for the `initialize` instruction.
/// Creates all global PDA accounts.
#[derive(Accounts)]
pub struct Initialize<'info> {
    /// The deployer who becomes the admin.
    #[account(mut)]
    pub admin: Signer<'info>,

    /// Global config PDA.
    #[account(
        init,
        payer = admin,
        space = RewardsConfig::LEN,
        seeds = [b"config"],
        bump,
    )]
    pub config: Account<'info, RewardsConfig>,

    /// Whitelist signers PDA.
    #[account(
        init,
        payer = admin,
        space = WhitelistSigners::space(10),
        seeds = [b"signers", config.key().as_ref()],
        bump,
    )]
    pub whitelist_signers: Account<'info, WhitelistSigners>,

    /// Token whitelist PDA.
    #[account(
        init,
        payer = admin,
        space = TokenWhitelist::space(50),
        seeds = [b"whitelist", config.key().as_ref()],
        bump,
    )]
    pub token_whitelist: Account<'info, TokenWhitelist>,

    /// Treasury state PDA.
    #[account(
        init,
        payer = admin,
        space = TreasuryState::LEN,
        seeds = [b"treasury", config.key().as_ref()],
        bump,
    )]
    pub treasury_state: Account<'info, TreasuryState>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<Initialize>,
    manager: Pubkey,
    minter: Pubkey,
    dev_config: Pubkey,
) -> Result<()> {
    // Validate no zero addresses
    require!(
        manager != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );
    require!(
        minter != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );
    require!(
        dev_config != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );

    // Initialize config
    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.manager = manager;
    config.minter = minter;
    config.dev_config = dev_config;
    config.is_paused = false;
    config.reward_token_count = 0;
    config.bump = ctx.bumps.config;

    // Initialize whitelist signers
    let signers = &mut ctx.accounts.whitelist_signers;
    signers.signers = Vec::new();
    signers.bump = ctx.bumps.whitelist_signers;

    // Initialize token whitelist
    let whitelist = &mut ctx.accounts.token_whitelist;
    whitelist.tokens = Vec::new();
    whitelist.bump = ctx.bumps.token_whitelist;

    // Initialize treasury state
    let treasury = &mut ctx.accounts.treasury_state;
    treasury.bump = ctx.bumps.treasury_state;

    emit!(ProgramInitialized {
        admin: ctx.accounts.admin.key(),
        manager,
        minter,
        dev_config,
    });

    Ok(())
}
