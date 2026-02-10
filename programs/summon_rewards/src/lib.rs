use anchor_lang::prelude::*;

pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("3XVtDWuE5Pgbu1bG2FNYBfkj51sdXQDXFg93rTnKUoJm");

#[program]
pub mod summon_rewards {
    use super::*;

    // ─── Admin / Configuration ───────────────────────────────────────

    pub fn initialize(
        ctx: Context<Initialize>,
        manager: Pubkey,
        minter: Pubkey,
        dev_config: Pubkey,
    ) -> Result<()> {
        instructions::initialize::handler(ctx, manager, minter, dev_config)
    }

    pub fn update_roles(
        ctx: Context<UpdateRoles>,
        role: RoleType,
        new_address: Pubkey,
    ) -> Result<()> {
        instructions::admin::update_roles_handler(ctx, role, new_address)
    }

    pub fn pause(ctx: Context<PauseUnpause>) -> Result<()> {
        instructions::admin::pause_handler(ctx)
    }

    pub fn unpause(ctx: Context<PauseUnpause>) -> Result<()> {
        instructions::admin::unpause_handler(ctx)
    }

    pub fn add_whitelist_signer(
        ctx: Context<ManageWhitelistSigner>,
        signer_to_add: Pubkey,
    ) -> Result<()> {
        instructions::admin::add_whitelist_signer_handler(ctx, signer_to_add)
    }

    pub fn remove_whitelist_signer(
        ctx: Context<ManageWhitelistSigner>,
        signer_to_remove: Pubkey,
    ) -> Result<()> {
        instructions::admin::remove_whitelist_signer_handler(ctx, signer_to_remove)
    }

    // ─── Treasury Management ─────────────────────────────────────────

    pub fn whitelist_token(
        ctx: Context<WhitelistToken>,
        mint: Pubkey,
        reward_type: state::RewardType,
    ) -> Result<()> {
        instructions::treasury::whitelist_token_handler(ctx, mint, reward_type)
    }

    pub fn remove_token_from_whitelist<'info>(
        ctx: Context<'_, '_, 'info, 'info, RemoveTokenFromWhitelist<'info>>,
        mint: Pubkey,
    ) -> Result<()> {
        instructions::treasury::remove_token_from_whitelist_handler(ctx, mint)
    }

    pub fn deposit_to_treasury(ctx: Context<DepositToTreasury>, amount: u64) -> Result<()> {
        instructions::treasury::deposit_to_treasury_handler(ctx, amount)
    }

    pub fn withdraw_unreserved_treasury(ctx: Context<WithdrawUnreservedTreasury>) -> Result<()> {
        instructions::treasury::withdraw_unreserved_treasury_handler(ctx)
    }

    pub fn deposit_nft_to_treasury(ctx: Context<DepositNftToTreasury>) -> Result<()> {
        instructions::treasury::deposit_nft_to_treasury_handler(ctx)
    }

    pub fn withdraw_unreserved_nft(ctx: Context<WithdrawUnreservedNft>) -> Result<()> {
        instructions::treasury::withdraw_unreserved_nft_handler(ctx)
    }

    // ─── Reservation Initialization ──────────────────────────────────

    pub fn init_token_reservation(
        ctx: Context<InitTokenReservation>,
        mint: Pubkey,
    ) -> Result<()> {
        instructions::reservation::init_token_reservation_handler(ctx, mint)
    }

    pub fn init_nft_reservation(
        ctx: Context<InitNftReservation>,
        nft_mint: Pubkey,
    ) -> Result<()> {
        instructions::reservation::init_nft_reservation_handler(ctx, nft_mint)
    }

    // ─── Reward Token Management ─────────────────────────────────────

    pub fn create_reward_token<'info>(
        ctx: Context<'_, '_, 'info, 'info, CreateRewardToken<'info>>,
        token_id: u64,
        token_uri: String,
        max_supply: u64,
        rewards: Vec<state::RewardEntry>,
    ) -> Result<()> {
        instructions::reward_token::create_reward_token_handler(
            ctx, token_id, token_uri, max_supply, rewards,
        )
    }

    pub fn update_token_mint_paused(
        ctx: Context<UpdateTokenPaused>,
        is_paused: bool,
    ) -> Result<()> {
        instructions::reward_token::update_token_mint_paused_handler(ctx, is_paused)
    }

    pub fn update_claim_paused(
        ctx: Context<UpdateTokenPaused>,
        is_paused: bool,
    ) -> Result<()> {
        instructions::reward_token::update_claim_paused_handler(ctx, is_paused)
    }

    pub fn increase_reward_supply<'info>(
        ctx: Context<'_, '_, 'info, 'info, IncreaseRewardSupply<'info>>,
        additional_supply: u64,
    ) -> Result<()> {
        instructions::reward_token::increase_reward_supply_handler(ctx, additional_supply)
    }

    pub fn update_token_uri(ctx: Context<UpdateTokenUri>, new_uri: String) -> Result<()> {
        instructions::reward_token::update_token_uri_handler(ctx, new_uri)
    }

    // ─── Access Token (Soulbound Token-2022) ────────────────────────

    pub fn create_access_token_mint(ctx: Context<CreateAccessTokenMint>) -> Result<()> {
        instructions::access_token::create_access_token_mint_handler(ctx)
    }

    pub fn mint_access_token(ctx: Context<MintAccessToken>, amount: u64) -> Result<()> {
        instructions::access_token::mint_access_token_handler(ctx, amount)
    }

    pub fn burn_access_token(ctx: Context<BurnAccessToken>, amount: u64) -> Result<()> {
        instructions::access_token::burn_access_token_handler(ctx, amount)
    }

    // ─── Minting ─────────────────────────────────────────────────────

    pub fn admin_mint(
        ctx: Context<AdminMint>,
        amount: u64,
        is_soulbound: bool,
    ) -> Result<()> {
        instructions::mint::admin_mint_handler(ctx, amount, is_soulbound)
    }

    pub fn mint_with_signature(
        ctx: Context<MintWithSignature>,
        nonce: u64,
        is_soulbound: bool,
        is_claim_reward: bool,
    ) -> Result<()> {
        instructions::mint::mint_with_signature_handler(ctx, nonce, is_soulbound, is_claim_reward)
    }

    // ─── Claiming ────────────────────────────────────────────────────

    pub fn claim_reward<'info>(
        ctx: Context<'_, '_, 'info, 'info, ClaimReward<'info>>,
    ) -> Result<()> {
        instructions::claim::claim_reward_handler(ctx)
    }

    pub fn admin_claim_reward<'info>(
        ctx: Context<'_, '_, 'info, 'info, AdminClaimReward<'info>>,
    ) -> Result<()> {
        instructions::claim::admin_claim_reward_handler(ctx)
    }
}
