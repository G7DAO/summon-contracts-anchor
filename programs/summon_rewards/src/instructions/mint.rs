use anchor_lang::prelude::*;
use anchor_lang::solana_program::ed25519_program;
use anchor_lang::solana_program::sysvar::instructions as instructions_sysvar;

use crate::errors::SummonRewardsError;
use crate::events::{Minted, UserNonceUsed};
use crate::state::{RewardsConfig, RewardTokenState, UserNonce, WhitelistSigners};

/// Accounts for `admin_mint`.
/// Mints access tokens to a recipient. Use `mint_access_token` for
/// the actual Token-2022 mint CPI after this instruction validates supply.
#[derive(Accounts)]
pub struct AdminMint<'info> {
    #[account(mut)]
    pub minter: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.minter == minter.key() @ SummonRewardsError::Unauthorized,
        constraint = !config.is_paused @ SummonRewardsError::ProgramPaused,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"reward_token", config.key().as_ref(), &reward_token_state.token_id.to_le_bytes()],
        bump = reward_token_state.bump,
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    pub system_program: Program<'info, System>,
}

/// Accounts for `mint_with_signature`.
/// User-initiated mint with Ed25519 signature verification.
///
/// The transaction must include an Ed25519 native program instruction
/// preceding this instruction. The handler verifies:
/// 1. An Ed25519 instruction exists in the transaction
/// 2. The signature was produced by a whitelisted signer
/// 3. The signed message matches the expected format
#[derive(Accounts)]
#[instruction(nonce: u64)]
pub struct MintWithSignature<'info> {
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
    )]
    pub reward_token_state: Account<'info, RewardTokenState>,

    #[account(
        seeds = [b"signers", config.key().as_ref()],
        bump = whitelist_signers.bump,
    )]
    pub whitelist_signers: Account<'info, WhitelistSigners>,

    /// User nonce PDA - init on first use to mark as consumed.
    /// The PDA init itself prevents nonce reuse (account already exists = error).
    #[account(
        init,
        payer = user,
        space = UserNonce::LEN,
        seeds = [b"nonce", config.key().as_ref(), user.key().as_ref(), &nonce.to_le_bytes()],
        bump,
    )]
    pub user_nonce: Account<'info, UserNonce>,

    /// Instructions sysvar for Ed25519 signature verification.
    /// CHECK: Validated by address constraint against sysvar::instructions::ID.
    #[account(address = instructions_sysvar::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

// ─── Handler Implementations ─────────────────────────────────────────

/// Admin mint access tokens to a recipient.
/// Maps from Rewards.sol: adminMintById / _mintAndClaimRewardToken
///
/// Validates supply limits, checks mint pause status, increments supply.
/// After this instruction, call `mint_access_token` to actually create the tokens.
pub fn admin_mint_handler(
    ctx: Context<AdminMint>,
    amount: u64,
    _is_soulbound: bool,
) -> Result<()> {
    require!(amount > 0, SummonRewardsError::InvalidAmount);

    let state = &mut ctx.accounts.reward_token_state;

    // Check mint is not paused for this token
    require!(!state.is_mint_paused, SummonRewardsError::MintPaused);

    // Check supply limits
    let new_supply = state
        .current_supply
        .checked_add(amount)
        .ok_or(SummonRewardsError::ArithmeticOverflow)?;
    require!(
        new_supply <= state.max_supply,
        SummonRewardsError::ExceedMaxSupply
    );

    // Increment current supply
    state.current_supply = new_supply;

    emit!(Minted {
        to: ctx.accounts.minter.key(),
        token_id: state.token_id,
        amount,
        soulbound: _is_soulbound,
    });

    Ok(())
}

/// Mint with Ed25519 signature verification.
/// Maps from Rewards.sol: mint (with signatureCheck modifier)
///
/// Verification flow:
/// 1. The nonce PDA init prevents nonce reuse (account-already-exists = replay protection)
/// 2. Inspect instructions sysvar for a preceding Ed25519 native program instruction
/// 3. Parse the Ed25519 instruction to extract the signer's public key
/// 4. Verify the signer is in the whitelist
/// 5. Validate the signed message contains the expected data
///
/// Expected signed message format: [user_pubkey(32) | token_id(8) | nonce(8)]
pub fn mint_with_signature_handler(
    ctx: Context<MintWithSignature>,
    nonce: u64,
    _is_soulbound: bool,
    _is_claim_reward: bool,
) -> Result<()> {
    let state = &mut ctx.accounts.reward_token_state;

    // Check mint is not paused for this token
    require!(!state.is_mint_paused, SummonRewardsError::MintPaused);

    // Check supply limits (mint 1 at a time, like Solidity)
    let new_supply = state
        .current_supply
        .checked_add(1)
        .ok_or(SummonRewardsError::ArithmeticOverflow)?;
    require!(
        new_supply <= state.max_supply,
        SummonRewardsError::ExceedMaxSupply
    );

    // ─── Ed25519 Signature Verification ──────────────────────────────

    // Build expected message: [user_pubkey(32) | token_id(8) | nonce(8)]
    let user_key = ctx.accounts.user.key();
    let token_id = state.token_id;
    let mut expected_msg = Vec::with_capacity(48);
    expected_msg.extend_from_slice(user_key.as_ref());
    expected_msg.extend_from_slice(&token_id.to_le_bytes());
    expected_msg.extend_from_slice(&nonce.to_le_bytes());

    // Verify Ed25519 signature by inspecting the instructions sysvar
    let signer_pubkey = verify_ed25519_signature(
        &ctx.accounts.instructions_sysvar,
        &expected_msg,
    )?;

    // Verify the signer is in the whitelist
    let is_whitelisted = ctx
        .accounts
        .whitelist_signers
        .signers
        .iter()
        .any(|s| *s == signer_pubkey);
    require!(is_whitelisted, SummonRewardsError::SignerNotWhitelisted);

    // ─── State Updates ───────────────────────────────────────────────

    // Mark nonce as used
    let user_nonce = &mut ctx.accounts.user_nonce;
    user_nonce.used = true;
    user_nonce.bump = ctx.bumps.user_nonce;

    // Increment current supply
    state.current_supply = new_supply;

    emit!(UserNonceUsed {
        user: user_key,
        nonce,
    });

    emit!(Minted {
        to: user_key,
        token_id: state.token_id,
        amount: 1,
        soulbound: _is_soulbound,
    });

    Ok(())
}

/// Verify that a preceding Ed25519 instruction exists in the transaction
/// and extract the signer's public key.
///
/// The Ed25519 native program instruction data format:
///   - Byte 0: num_signatures (u8)
///   - Byte 1: padding (u8)
///   - Then for each signature, Ed25519SignatureOffsets (14 bytes):
///     - Byte 0-1: signature_offset (u16 LE)
///     - Byte 2-3: signature_instruction_index (u16 LE)
///     - Byte 4-5: public_key_offset (u16 LE)
///     - Byte 6-7: public_key_instruction_index (u16 LE)
///     - Byte 8-9: message_data_offset (u16 LE)
///     - Byte 10-11: message_data_size (u16 LE)
///     - Byte 12-13: message_instruction_index (u16 LE)
///   Then inline data: signature(64) + pubkey(32) + message(variable)
fn verify_ed25519_signature(
    instructions_sysvar: &AccountInfo,
    expected_message: &[u8],
) -> Result<Pubkey> {
    // Get the current instruction index
    let current_ix_index =
        instructions_sysvar::load_current_index_checked(instructions_sysvar)
            .map_err(|_| SummonRewardsError::MissingEd25519Instruction)?;

    // We expect the Ed25519 instruction to be immediately before this one
    require!(
        current_ix_index > 0,
        SummonRewardsError::MissingEd25519Instruction
    );

    let ed25519_ix = instructions_sysvar::load_instruction_at_checked(
        (current_ix_index - 1) as usize,
        instructions_sysvar,
    )
    .map_err(|_| SummonRewardsError::MissingEd25519Instruction)?;

    // Verify this is the Ed25519 native program
    require!(
        ed25519_ix.program_id == ed25519_program::ID,
        SummonRewardsError::MissingEd25519Instruction
    );

    // Parse the Ed25519 instruction data
    // Format: num_signatures(u8) + padding(u8) + Ed25519SignatureOffsets[](14 bytes each) + data
    let ix_data = &ed25519_ix.data;
    require!(
        ix_data.len() >= 2,
        SummonRewardsError::InvalidSignature
    );

    // Number of signatures (u8 at byte 0, byte 1 is padding)
    let num_signatures = ix_data[0];
    require!(
        num_signatures >= 1,
        SummonRewardsError::InvalidSignature
    );

    // Parse the first signature entry (offsets start at byte 2 after num_signatures(1) + padding(1))
    // Ed25519SignatureOffsets struct is 14 bytes
    let offsets_start = 2; // after num_signatures(1) + padding(1)
    require!(
        ix_data.len() >= offsets_start + 14,
        SummonRewardsError::InvalidSignature
    );

    let public_key_offset = u16::from_le_bytes([
        ix_data[offsets_start + 4],
        ix_data[offsets_start + 5],
    ]) as usize;

    let message_data_offset = u16::from_le_bytes([
        ix_data[offsets_start + 8],
        ix_data[offsets_start + 9],
    ]) as usize;

    let message_data_size = u16::from_le_bytes([
        ix_data[offsets_start + 10],
        ix_data[offsets_start + 11],
    ]) as usize;

    // Extract public key (32 bytes)
    require!(
        ix_data.len() >= public_key_offset + 32,
        SummonRewardsError::InvalidSignature
    );
    let signer_pubkey = Pubkey::try_from(&ix_data[public_key_offset..public_key_offset + 32])
        .map_err(|_| SummonRewardsError::InvalidSignature)?;

    // Extract and verify message
    require!(
        ix_data.len() >= message_data_offset + message_data_size,
        SummonRewardsError::InvalidSignature
    );
    let signed_message = &ix_data[message_data_offset..message_data_offset + message_data_size];

    // Verify the signed message matches our expected message
    require!(
        signed_message == expected_message,
        SummonRewardsError::InvalidSignature
    );

    Ok(signer_pubkey)
}
