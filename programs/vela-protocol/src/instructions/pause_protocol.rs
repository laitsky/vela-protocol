use anchor_lang::prelude::*;

use crate::{errors::VelaError, state::ProtocolConfig};

#[derive(Accounts)]
pub struct PauseProtocol<'info> {
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

pub fn handler(ctx: Context<PauseProtocol>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.paused = true;
    config.paused_at = Clock::get()?.unix_timestamp;
    Ok(())
}
