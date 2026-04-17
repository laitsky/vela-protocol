use crate::errors::VelaError;
use crate::instructions::keeper_config_account::initialize_keeper_config;
use crate::instructions::protocol_config_account::load_protocol_config;
use crate::state::{KeeperConfig, KeeperMode, ProtocolConfig};
use anchor_lang::prelude::*;

pub fn handler(
    ctx: Context<InitKeeperConfig>,
    mode: KeeperMode,
    keeper_endpoint: Vec<u8>,
    keeper_authority: Pubkey,
) -> Result<()> {
    require!(keeper_endpoint.len() <= 128, VelaError::EndpointTooLong);
    require!(!keeper_endpoint.is_empty(), VelaError::EndpointEmpty);
    require!(
        keeper_authority != Pubkey::default(),
        VelaError::InvalidKeeperAuthority
    );

    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require_keys_eq!(
        protocol_config.admin(),
        ctx.accounts.admin.key(),
        VelaError::UnauthorizedAdmin
    );

    let config = initialize_keeper_config(
        &ctx.accounts.admin.to_account_info(),
        &ctx.accounts.keeper_config.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        ctx.accounts.admin.key(),
        mode.clone(),
        keeper_endpoint,
        keeper_authority,
        ctx.bumps.keeper_config,
    )?;

    emit!(KeeperConfigInitialized {
        admin: config.admin,
        mode: config.mode,
        keeper_authority: config.keeper_authority,
    });

    Ok(())
}

#[event]
pub struct KeeperConfigInitialized {
    pub admin: Pubkey,
    pub mode: KeeperMode,
    pub keeper_authority: Pubkey,
}

#[derive(Accounts)]
pub struct InitKeeperConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [KeeperConfig::SEED_PREFIX],
        bump,
    )]
    pub keeper_config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
