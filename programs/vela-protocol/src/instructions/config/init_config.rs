use crate::errors::VelaError;
use crate::instructions::arcium_accounts::validate_cluster_configuration;
use crate::instructions::protocol_config_account::{
    load_protocol_config, upgrade_protocol_config, write_protocol_config,
};
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
    **config = ProtocolConfig::new(
        ctx.accounts.admin.key(),
        ix.cluster_pubkey,
        ix.cluster_type,
        ix.cluster_offset,
        ctx.bumps.config,
    );
    Ok(())
}

pub fn update_config(ctx: Context<UpdateConfig>, ix: UpdateConfigIx) -> Result<()> {
    validate_cluster_configuration(&ix.cluster_pubkey, ix.cluster_offset)?;
    let existing = load_protocol_config(&ctx.accounts.config.to_account_info())?;
    require_keys_eq!(
        ctx.accounts.admin.key(),
        existing.admin(),
        VelaError::UnauthorizedAdmin
    );
    let mut config = upgrade_protocol_config(
        &ctx.accounts.admin.to_account_info(),
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
    )?;
    config.cluster_pubkey = ix.cluster_pubkey;
    config.cluster_type = ix.cluster_type;
    config.cluster_offset = ix.cluster_offset;
    write_protocol_config(&ctx.accounts.config.to_account_info(), &config)?;
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
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
