use crate::instructions::arcium_accounts::validate_cluster_configuration;
use crate::errors::VelaError;
use crate::state::{ClusterType, ProtocolConfig};
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitConfigIx {
    pub cluster_pubkey: Pubkey,
    pub cluster_type: ClusterType,
    pub cluster_offset: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdateConfigIx {
    pub cluster_pubkey: Pubkey,
    pub cluster_type: ClusterType,
    pub cluster_offset: u64,
}

pub fn init_config(ctx: Context<InitConfig>, ix: InitConfigIx) -> Result<()> {
    validate_cluster_configuration(&ix.cluster_pubkey, ix.cluster_offset)?;
    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.cluster_pubkey = ix.cluster_pubkey;
    config.cluster_type = ix.cluster_type;
    config.cluster_offset = ix.cluster_offset;
    config.bump = ctx.bumps.config;
    Ok(())
}

pub fn update_config(ctx: Context<UpdateConfig>, ix: UpdateConfigIx) -> Result<()> {
    validate_cluster_configuration(&ix.cluster_pubkey, ix.cluster_offset)?;
    let config = &mut ctx.accounts.config;
    config.cluster_pubkey = ix.cluster_pubkey;
    config.cluster_type = ix.cluster_type;
    config.cluster_offset = ix.cluster_offset;
    Ok(())
}

#[derive(Accounts)]
pub struct InitConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = ProtocolConfig::SIZE,
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: Account<'info, ProtocolConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    #[account(
        constraint = admin.key() == config.admin @ VelaError::UnauthorizedAdmin
    )]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = config.bump,
    )]
    pub config: Account<'info, ProtocolConfig>,
}
