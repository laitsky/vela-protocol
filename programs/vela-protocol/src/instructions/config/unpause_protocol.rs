use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::protocol_config_account::{
        load_protocol_config, upgrade_protocol_config, write_protocol_config,
    },
    state::ProtocolConfig,
};

#[derive(Accounts)]
pub struct UnpauseProtocol<'info> {
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<UnpauseProtocol>) -> Result<()> {
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
    config.paused = false;
    config.paused_at = 0;
    write_protocol_config(&ctx.accounts.config.to_account_info(), &config)?;
    Ok(())
}
