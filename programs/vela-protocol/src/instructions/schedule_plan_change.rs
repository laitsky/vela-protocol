use anchor_lang::prelude::*;

use crate::{
    constants::WRAPPED_USDC_SYMBOL,
    errors::VelaError,
    instructions::{
        mandate_account::{load_mandate_account, validate_loaded_mandate_address, write_mandate},
        plan_account::load_plan_account,
        protocol_config_account::load_protocol_config,
    },
    state::{MandateStatus, MandateUpgradeInitiated, ProtocolConfig},
};

#[derive(Accounts)]
pub struct SchedulePlanChange<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Deserialized and validated manually to support flat and usage plans.
    pub new_plan: UncheckedAccount<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<SchedulePlanChange>) -> Result<()> {
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require!(!protocol_config.paused(), VelaError::ProtocolPaused);

    let loaded_mandate = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    require!(!loaded_mandate.is_legacy(), VelaError::MandateVersionUnsupported);
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &loaded_mandate)?;
    let mut mandate = loaded_mandate.into_current();
    require!(
        matches!(mandate.status, MandateStatus::Active),
        VelaError::MandateNotActive
    );

    let new_plan = load_plan_account(&ctx.accounts.new_plan.to_account_info())?;
    require_keys_eq!(
        new_plan.merchant(),
        mandate.merchant,
        VelaError::UnauthorizedUpgrade
    );
    require!(
        matches!(new_plan.status(), crate::state::PlanStatus::Active),
        VelaError::PlanNotActive
    );

    let authority = ctx.accounts.authority.key();
    let is_subscriber = authority == mandate.subscriber;
    let is_merchant = authority == mandate.merchant;
    require!(
        is_subscriber || is_merchant,
        VelaError::UnauthorizedUpgrade
    );
    if new_plan.mandate_amount() > mandate.amount {
        require!(is_subscriber, VelaError::UnauthorizedUpgrade);
    }

    let new_plan_key = ctx.accounts.new_plan.key();
    if mandate.plan == new_plan_key {
        return Ok(());
    }

    let nonce = compute_nonce(&ctx.accounts.mandate.key(), &new_plan_key, Clock::get()?.slot);
    if mandate.pending_change_type == 2
        && mandate.pending_new_plan == new_plan_key
        && mandate.pending_nonce_short == nonce
    {
        return Ok(());
    }

    let applied_at = mandate.next_payment_due;
    mandate.pending_new_plan = new_plan_key;
    mandate.pending_effective_at = applied_at;
    mandate.pending_change_type = 2;
    mandate.pending_nonce_short = nonce;
    mandate.version = crate::state::CURRENT_ACCOUNT_VERSION;
    write_mandate(&ctx.accounts.mandate.to_account_info(), &mandate, false)?;

    let timestamp = Clock::get()?.unix_timestamp;
    emit!(MandateUpgradeInitiated {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        mint: protocol_config.wrapped_usdc_mint(),
        token_symbol: WRAPPED_USDC_SYMBOL.to_string(),
        old_plan: mandate.plan,
        new_plan: new_plan_key,
        proration_amount: 0,
        change_type: 2,
        signer: authority,
        applied_at,
        timestamp,
    });

    Ok(())
}

fn compute_nonce(mandate: &Pubkey, new_plan: &Pubkey, slot: u64) -> [u8; 8] {
    let _ = (mandate, new_plan);
    slot.to_le_bytes()
}
