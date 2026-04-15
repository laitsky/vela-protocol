use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::{
        mandate_account::{load_mandate_account, validate_loaded_mandate_address, write_mandate},
        protocol_config_account::load_protocol_config,
    },
    state::{MandateStatus, MandateUpgradeCancelled, ProtocolConfig},
};

#[derive(Accounts)]
pub struct CancelPlanChange<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<CancelPlanChange>) -> Result<()> {
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;

    let loaded_mandate = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    require!(!loaded_mandate.is_legacy(), VelaError::MandateVersionUnsupported);
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &loaded_mandate)?;
    let mut mandate = loaded_mandate.into_current();
    require!(
        matches!(mandate.status, MandateStatus::Active),
        VelaError::MandateNotActive
    );
    require!(mandate.has_pending_change(), VelaError::NoPendingChange);

    let authority = ctx.accounts.authority.key();
    require!(
        authority == mandate.subscriber || authority == mandate.merchant,
        VelaError::UnauthorizedUpgrade
    );

    let cancelled_plan = mandate.pending_new_plan;
    mandate.clear_pending();
    mandate.version = crate::state::CURRENT_ACCOUNT_VERSION;
    write_mandate(&ctx.accounts.mandate.to_account_info(), &mandate, false)?;

    let timestamp = Clock::get()?.unix_timestamp;
    emit!(MandateUpgradeCancelled {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        mint: protocol_config.wrapped_usdc_mint(),
        cancelled_plan,
        signer: authority,
        timestamp,
    });

    Ok(())
}
