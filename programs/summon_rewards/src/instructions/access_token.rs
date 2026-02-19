use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token_2022::Token2022;

use crate::errors::SummonRewardsError;
use crate::state::{RewardsConfig, RewardTokenState};

/// Accounts for `create_access_token_mint`.
/// Creates a Token-2022 mint with NonTransferable extension (soulbound).
/// Maps from AccessToken.sol: addNewToken
#[derive(Accounts)]
pub struct CreateAccessTokenMint<'info> {
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
        mut,
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
        constraint = reward_token_state.access_token_mint == Pubkey::default() @ SummonRewardsError::DupTokenId,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    /// The Token-2022 mint to create. PDA with seeds ["access_mint", config, token_id].
    /// CHECK: Will be initialized as a Token-2022 mint with NonTransferable extension.
    #[account(
        mut,
        seeds = [b"access_mint", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump,
    )]
    pub access_token_mint: AccountInfo<'info>,

    pub token_program_2022: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

/// Accounts for `mint_access_token`.
/// Mints access tokens to a user via Token-2022 CPI.
/// Called by admin_mint and mint_with_signature flows.
#[derive(Accounts)]
pub struct MintAccessToken<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.minter == authority.key() @ SummonRewardsError::Unauthorized,
        constraint = !config.is_paused @ SummonRewardsError::ProgramPaused,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
        constraint = reward_token_state.access_token_mint != Pubkey::default() @ SummonRewardsError::TokenNotExist,
        constraint = !reward_token_state.is_mint_paused @ SummonRewardsError::MintPaused,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    /// The Token-2022 access token mint.
    /// CHECK: Verified via constraint against reward_token_state.access_token_mint.
    #[account(
        mut,
        constraint = access_token_mint.key() == reward_token_state.access_token_mint @ SummonRewardsError::InvalidInput,
    )]
    pub access_token_mint: AccountInfo<'info>,

    /// Recipient's Token-2022 token account.
    /// CHECK: Validated by Token-2022 program during MintTo CPI.
    #[account(mut)]
    pub recipient_token_account: AccountInfo<'info>,

    pub token_program_2022: Program<'info, Token2022>,
}

/// Accounts for `burn_access_token`.
/// Burns access tokens during claim. Called internally by claim flow.
#[derive(Accounts)]
pub struct BurnAccessToken<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = !config.is_paused @ SummonRewardsError::ProgramPaused,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
        constraint = reward_token_state.access_token_mint != Pubkey::default() @ SummonRewardsError::TokenNotExist,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    /// The Token-2022 access token mint.
    /// CHECK: Verified via constraint against reward_token_state.access_token_mint.
    #[account(
        mut,
        constraint = access_token_mint.key() == reward_token_state.access_token_mint @ SummonRewardsError::InvalidInput,
    )]
    pub access_token_mint: AccountInfo<'info>,

    /// Holder's Token-2022 token account.
    /// CHECK: Validated by Token-2022 program during Burn CPI.
    #[account(mut)]
    pub holder_token_account: AccountInfo<'info>,

    pub token_program_2022: Program<'info, Token2022>,
}

// ─── Handler Implementations ─────────────────────────────────────────

/// Create a Token-2022 mint with NonTransferable extension for soulbound access tokens.
/// Maps from AccessToken.sol: addNewToken
///
/// Steps:
/// 1. Allocate space for Token-2022 mint with NonTransferable extension
/// 2. Initialize NonTransferable extension
/// 3. Initialize mint with config PDA as mint authority (decimals=0)
pub fn create_access_token_mint_handler(ctx: Context<CreateAccessTokenMint>) -> Result<()> {
    let config_key = ctx.accounts.config.key();
    let token_id = ctx.accounts.reward_token_state.token_id;
    let access_mint_bump = ctx.bumps.access_token_mint;

    let signer_seeds: &[&[&[u8]]] = &[&[
        b"access_mint",
        config_key.as_ref(),
        &token_id.to_le_bytes(),
        &[access_mint_bump],
    ]];

    let mint_account = &ctx.accounts.access_token_mint;
    let token_program = &ctx.accounts.token_program_2022;

    // Calculate mint space with NonTransferable extension
    let extension_types = vec![spl_token_2022::extension::ExtensionType::NonTransferable];
    let mint_len = spl_token_2022::extension::ExtensionType::try_calculate_account_len::<
        spl_token_2022::state::Mint,
    >(&extension_types)?;

    // Create account for the mint
    let rent = Rent::get()?;
    let lamports = rent.minimum_balance(mint_len);

    anchor_lang::solana_program::program::invoke_signed(
        &anchor_lang::solana_program::system_instruction::create_account(
            &ctx.accounts.manager.key(),
            &mint_account.key(),
            lamports,
            mint_len as u64,
            &token_program.key(),
        ),
        &[
            ctx.accounts.manager.to_account_info(),
            mint_account.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        signer_seeds,
    )?;

    // Initialize NonTransferable extension (must be done BEFORE InitializeMint)
    let init_non_transferable_ix =
        spl_token_2022::instruction::initialize_non_transferable_mint(
            &token_program.key(),
            &mint_account.key(),
        )?;

    invoke_signed(
        &init_non_transferable_ix,
        &[mint_account.to_account_info()],
        signer_seeds,
    )?;

    // Initialize Mint (decimals=0, mint authority = config PDA)
    let init_mint_ix = spl_token_2022::instruction::initialize_mint2(
        &token_program.key(),
        &mint_account.key(),
        &config_key,  // mint authority = config PDA
        None,         // no freeze authority
        0,            // decimals = 0 (like ERC1155 with integer amounts)
    )?;

    invoke_signed(
        &init_mint_ix,
        &[mint_account.to_account_info()],
        signer_seeds,
    )?;

    // Store the mint pubkey in reward token state
    let state = &mut ctx.accounts.reward_token_state;
    state.access_token_mint = mint_account.key();

    Ok(())
}

/// Mint access tokens to a recipient via Token-2022 CPI.
/// Maps from AccessToken.sol: adminMintId
///
/// The config PDA is the mint authority, so we sign with its seeds.
/// Supply is tracked: new_supply = current_supply + amount must not exceed max_supply.
pub fn mint_access_token_handler(
    ctx: Context<MintAccessToken>,
    amount: u64,
) -> Result<()> {
    require!(amount > 0, SummonRewardsError::InvalidAmount);

    // ── Supply check ─────────────────────────────────────────────────
    let state = &mut ctx.accounts.reward_token_state;
    let new_supply = state
        .current_supply
        .checked_add(amount)
        .ok_or(SummonRewardsError::ArithmeticOverflow)?;
    require!(
        new_supply <= state.max_supply,
        SummonRewardsError::ExceedMaxSupply
    );
    state.current_supply = new_supply;

    // ── CPI: MintTo via Token-2022 ───────────────────────────────────
    // Config PDA is the mint authority
    let config_bump = ctx.accounts.config.bump;
    let config_signer_seeds: &[&[&[u8]]] = &[&[b"config", &[config_bump]]];

    let mint_to_ix = spl_token_2022::instruction::mint_to(
        &ctx.accounts.token_program_2022.key(),
        &ctx.accounts.access_token_mint.key(),
        &ctx.accounts.recipient_token_account.key(),
        &ctx.accounts.config.key(), // mint authority
        &[],
        amount,
    )?;

    invoke_signed(
        &mint_to_ix,
        &[
            ctx.accounts.access_token_mint.to_account_info(),
            ctx.accounts.recipient_token_account.to_account_info(),
            ctx.accounts.config.to_account_info(),
        ],
        config_signer_seeds,
    )?;

    Ok(())
}

/// Burn access tokens during claim via Token-2022 CPI.
/// Maps from AccessToken.sol: whitelistBurn
///
/// The holder (user) is the authority for the burn.
pub fn burn_access_token_handler(
    ctx: Context<BurnAccessToken>,
    amount: u64,
) -> Result<()> {
    require!(amount > 0, SummonRewardsError::InvalidAmount);

    // CPI: Burn via Token-2022
    let burn_ix = spl_token_2022::instruction::burn(
        &ctx.accounts.token_program_2022.key(),
        &ctx.accounts.holder_token_account.key(),
        &ctx.accounts.access_token_mint.key(),
        &ctx.accounts.holder.key(), // authority = token holder
        &[],
        amount,
    )?;

    anchor_lang::solana_program::program::invoke(
        &burn_ix,
        &[
            ctx.accounts.holder_token_account.to_account_info(),
            ctx.accounts.access_token_mint.to_account_info(),
            ctx.accounts.holder.to_account_info(),
        ],
    )?;

    Ok(())
}
