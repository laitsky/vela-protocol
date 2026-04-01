use crate::errors::VelaError;
use crate::state::{KeeperConfig, KeeperMode, ProtocolConfig};
use anchor_lang::prelude::*;

pub fn handler(
    ctx: Context<InitKeeperConfig>,
    mode: KeeperMode,
    keeper_endpoint: Vec<u8>,
    keeper_authority: Pubkey,
) -> Result<()> {
    require!(
        keeper_endpoint.len() <= 128,
        VelaError::EndpointTooLong
    );

    let config = &mut ctx.accounts.keeper_config;
    config.admin = ctx.accounts.admin.key();
    config.mode = mode;
    config.endpoint_len = keeper_endpoint.len() as u8;
    config.keeper_endpoint = [0u8; 128];
    config.keeper_endpoint[..keeper_endpoint.len()].copy_from_slice(&keeper_endpoint);
    config.keeper_authority = keeper_authority;
    config.bump = ctx.bumps.keeper_config;

    Ok(())
}

#[derive(Accounts)]
pub struct InitKeeperConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = protocol_config.bump,
        constraint = protocol_config.admin == admin.key() @ VelaError::UnauthorizedAdmin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    #[account(
        init,
        payer = admin,
        space = KeeperConfig::SIZE,
        seeds = [KeeperConfig::SEED_PREFIX],
        bump,
    )]
    pub keeper_config: Account<'info, KeeperConfig>,
    pub system_program: Program<'info, System>,
}
