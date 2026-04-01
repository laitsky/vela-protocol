use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    state::{BillingType, MandateStatus, VelaMandate, UsageReport},
};

#[derive(Accounts)]
#[instruction(period_start: i64)]
pub struct SubmitUsageReport<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(
        seeds = [
            VelaMandate::SEED_PREFIX,
            mandate.subscriber.as_ref(),
            mandate.plan.as_ref()
        ],
        bump = mandate.bump
    )]
    pub mandate: Account<'info, VelaMandate>,

    #[account(
        init,
        payer = merchant,
        space = UsageReport::SIZE,
        seeds = [
            UsageReport::SEED_PREFIX,
            mandate.key().as_ref(),
            period_start.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub usage_report: Account<'info, UsageReport>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<SubmitUsageReport>,
    period_start: i64,
    period_end: i64,
    encrypted_usage: [[u8; 32]; 4],
    nonce: u128,
    pub_key: [u8; 32],
) -> Result<()> {
    let mandate = &ctx.accounts.mandate;

    // Validate merchant authorization
    require_keys_eq!(
        ctx.accounts.merchant.key(),
        mandate.merchant,
        VelaError::UnauthorizedKeeper
    );

    // Validate billing type is Usage
    require!(
        mandate.billing_type == BillingType::Usage,
        VelaError::BillingTypeMismatch
    );

    // Validate mandate is active
    require!(
        mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );

    // Validate period boundaries
    require!(period_start < period_end, VelaError::InvalidPeriod);

    // Validate period_start aligns with mandate.next_payment_due (Pitfall 5)
    require!(
        period_start == mandate.next_payment_due,
        VelaError::PeriodMismatch
    );

    let clock = Clock::get()?;

    ctx.accounts.usage_report.set_inner(UsageReport {
        mandate: mandate.key(),
        merchant: mandate.merchant,
        period_start,
        period_end,
        encrypted_usage,
        nonce,
        pub_key,
        settled: false,
        created_at: clock.unix_timestamp,
        bump: ctx.bumps.usage_report,
    });

    Ok(())
}
