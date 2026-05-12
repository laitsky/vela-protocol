use anchor_lang::prelude::*;

use crate::{
    constants::MAX_USAGE_COMPUTATION_CIPHERTEXTS,
    errors::VelaError,
    instructions::{
        create_usage_plan::validate_usage_pricing_bounds,
        mandate_account::{
            load_mandate_account, mandate_billing_period, validate_loaded_mandate_address,
        },
        plan_account::{load_plan_account, require_plan_billing_type, LoadedPlanAccount},
    },
    state::{BillingType, MandateStatus, PlanStatus, UsageReport},
};

const USAGE_UNITS_CIPHERTEXT_COUNT: usize = 1;

#[derive(Accounts)]
#[instruction(period_start: i64)]
pub struct SubmitUsageReport<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    /// CHECK: Deserialized and validated manually to support both legacy and V2 mandate layouts.
    pub mandate: UncheckedAccount<'info>,

    /// CHECK: Deserialized and validated manually to support current and legacy usage plans.
    pub usage_plan: UncheckedAccount<'info>,

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
    computation_ciphertext: Vec<[u8; 32]>,
    nonce: u128,
    pub_key: [u8; 32],
) -> Result<()> {
    let loaded = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &loaded)?;
    let mandate = loaded.into_current();

    // Production usage reports are merchant-authored. The report PDA is keyed only by
    // mandate and period, so allowing subscribers here would let them preempt the
    // merchant's canonical report for the billing period.
    require!(
        ctx.accounts.merchant.key() == mandate.merchant,
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
    require_keys_eq!(
        ctx.accounts.usage_plan.key(),
        mandate.plan,
        VelaError::PlanNotActive
    );

    let plan = load_plan_account(&ctx.accounts.usage_plan.to_account_info())?;
    require_plan_billing_type(&plan, &BillingType::Usage)?;
    require!(
        *plan.status() == PlanStatus::Active,
        VelaError::PlanNotActive
    );
    require_keys_eq!(
        plan.merchant(),
        mandate.merchant,
        VelaError::BillingTypeMismatch
    );

    let tier_count = plan
        .usage_tier_count()
        .ok_or(VelaError::BillingTypeMismatch)?;
    require!(
        tier_count > 0 && tier_count <= 5,
        VelaError::InvalidTierCount
    );
    validate_loaded_usage_pricing_bounds(&plan)?;
    require!(
        computation_ciphertext.len() == USAGE_UNITS_CIPHERTEXT_COUNT,
        VelaError::InvalidCiphertextInput
    );

    let (expected_period_start, expected_period_end) = mandate_billing_period(&mandate)?;
    require!(period_start < period_end, VelaError::InvalidPeriod);
    require!(
        period_start == expected_period_start && period_end == expected_period_end,
        VelaError::PeriodMismatch
    );

    let clock = Clock::get()?;
    require!(clock.unix_timestamp >= period_end, VelaError::PullTooEarly);
    let mut ciphertext_array = [[0u8; 32]; MAX_USAGE_COMPUTATION_CIPHERTEXTS];
    ciphertext_array[0] = computation_ciphertext[0];
    ciphertext_array[1] = plan.usage_terms_hash(&ctx.accounts.usage_plan.key())?;

    ctx.accounts.usage_report.set_inner(UsageReport {
        mandate: ctx.accounts.mandate.key(),
        merchant: mandate.merchant,
        period_start,
        period_end,
        computation_ciphertext: ciphertext_array,
        ciphertext_count: USAGE_UNITS_CIPHERTEXT_COUNT as u8,
        nonce,
        pub_key,
        settled: false,
        created_at: clock.unix_timestamp,
        bump: ctx.bumps.usage_report,
    });

    Ok(())
}

fn validate_loaded_usage_pricing_bounds(plan: &LoadedPlanAccount) -> Result<()> {
    match plan {
        LoadedPlanAccount::Usage(plan) => {
            require!(
                plan.tier_count > 0 && plan.tier_count <= 5,
                VelaError::InvalidTierCount
            );
            validate_usage_pricing_bounds(
                &plan.tiers[..usize::from(plan.tier_count)],
                plan.max_charge_per_period,
            )
        }
        LoadedPlanAccount::LegacyUsage(plan) => {
            require!(
                plan.tier_count > 0 && plan.tier_count <= 5,
                VelaError::InvalidTierCount
            );
            validate_usage_pricing_bounds(
                &plan.tiers[..usize::from(plan.tier_count)],
                plan.max_charge_per_period,
            )
        }
        LoadedPlanAccount::Flat(_) | LoadedPlanAccount::LegacyFlat(_) => {
            Err(VelaError::BillingTypeMismatch.into())
        }
    }
}
