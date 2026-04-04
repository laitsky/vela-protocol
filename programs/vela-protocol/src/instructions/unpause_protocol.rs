use anchor_lang::prelude::*;

use crate::{errors::VelaError, state::ProtocolConfig};

#[derive(Accounts)]
pub struct UnpauseProtocol<'info> {
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

pub fn handler(ctx: Context<UnpauseProtocol>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.paused = false;
    config.paused_at = 0;
    Ok(())
}
