use anchor_lang::prelude::*;

use crate::errors::SummonRewardsError;
use crate::events::{RoleUpdated, WhitelistSignerAdded, WhitelistSignerRemoved};
use crate::state::{RewardsConfig, WhitelistSigners};

/// Role type enum for the `update_roles` instruction.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub enum RoleType {
    Manager,
    Minter,
    DevConfig,
    Admin,
}

/// Accounts for `update_roles`.
#[derive(Accounts)]
pub struct UpdateRoles<'info> {
    /// Must be the current admin.
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.admin == admin.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,
}

/// Accounts for `pause` / `unpause`.
#[derive(Accounts)]
pub struct PauseUnpause<'info> {
    /// Must be the manager.
    pub manager: Signer<'info>,

    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.manager == manager.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,
}

/// Accounts for `add_whitelist_signer` / `remove_whitelist_signer`.
#[derive(Accounts)]
pub struct ManageWhitelistSigner<'info> {
    /// Must be the dev_config role.
    pub authority: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
        constraint = config.dev_config == authority.key() @ SummonRewardsError::Unauthorized,
    )]
    pub config: Account<'info, RewardsConfig>,

    #[account(
        mut,
        seeds = [b"signers", config.key().as_ref()],
        bump = whitelist_signers.bump,
    )]
    pub whitelist_signers: Account<'info, WhitelistSigners>,
}

pub fn update_roles_handler(
    ctx: Context<UpdateRoles>,
    role: RoleType,
    new_address: Pubkey,
) -> Result<()> {
    require!(
        new_address != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );

    let config = &mut ctx.accounts.config;
    let (role_name, old_address) = match role {
        RoleType::Manager => {
            let old = config.manager;
            config.manager = new_address;
            ("manager", old)
        }
        RoleType::Minter => {
            let old = config.minter;
            config.minter = new_address;
            ("minter", old)
        }
        RoleType::DevConfig => {
            let old = config.dev_config;
            config.dev_config = new_address;
            ("dev_config", old)
        }
        RoleType::Admin => {
            let old = config.admin;
            config.admin = new_address;
            ("admin", old)
        }
    };

    emit!(RoleUpdated {
        role: role_name.to_string(),
        old_address,
        new_address,
    });

    Ok(())
}

pub fn pause_handler(ctx: Context<PauseUnpause>) -> Result<()> {
    ctx.accounts.config.is_paused = true;
    Ok(())
}

pub fn unpause_handler(ctx: Context<PauseUnpause>) -> Result<()> {
    ctx.accounts.config.is_paused = false;
    Ok(())
}

pub fn add_whitelist_signer_handler(
    ctx: Context<ManageWhitelistSigner>,
    signer_to_add: Pubkey,
) -> Result<()> {
    require!(
        signer_to_add != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );

    let signers = &mut ctx.accounts.whitelist_signers;

    // Check not already added
    require!(
        !signers.signers.contains(&signer_to_add),
        SummonRewardsError::SignerAlreadyWhitelisted
    );

    signers.signers.push(signer_to_add);

    emit!(WhitelistSignerAdded {
        signer: signer_to_add,
    });

    Ok(())
}

pub fn remove_whitelist_signer_handler(
    ctx: Context<ManageWhitelistSigner>,
    signer_to_remove: Pubkey,
) -> Result<()> {
    require!(
        signer_to_remove != Pubkey::default(),
        SummonRewardsError::AddressIsZero
    );

    let signers = &mut ctx.accounts.whitelist_signers;

    // Find and remove
    let pos = signers
        .signers
        .iter()
        .position(|s| *s == signer_to_remove)
        .ok_or(SummonRewardsError::SignerNotWhitelisted)?;

    signers.signers.swap_remove(pos);

    emit!(WhitelistSignerRemoved {
        signer: signer_to_remove,
    });

    Ok(())
}
