use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::{
        create_usage_plan::validate_usage_pricing_bounds,
        plan_account::{load_plan_account, write_usage_plan, LoadedPlanAccount},
    },
    state::{PricingTier, UsagePlan},
};

#[event]
pub struct UsagePlanUpdated {
    pub plan: Pubkey,
    pub merchant: Pubkey,
    pub plan_id: u64,
    pub old_max_charge_per_period: u64,
    pub new_max_charge_per_period: u64,
    pub old_settlement_frequency: u64,
    pub new_settlement_frequency: u64,
    pub old_tier_count: u8,
    pub new_tier_count: u8,
}

#[derive(Accounts)]
pub struct UpdateUsagePlan<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(mut)]
    pub usage_plan: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<UpdateUsagePlan>,
    tiers: Option<Vec<PricingTier>>,
    max_charge_per_period: Option<u64>,
    settlement_frequency: Option<u64>,
) -> Result<()> {
    require!(
        tiers.is_some() || max_charge_per_period.is_some() || settlement_frequency.is_some(),
        VelaError::NoUpdateProvided,
    );

    let plan_info = ctx.accounts.usage_plan.to_account_info();
    let merchant_key = ctx.accounts.merchant.key();

    let loaded = load_plan_account(&plan_info)?;

    // Must NOT be a flat plan
    require!(!loaded.is_flat(), VelaError::BillingTypeMismatch);

    // Must be the plan owner
    require!(
        loaded.merchant() == merchant_key,
        VelaError::UnauthorizedCancel,
    );

    // Convert to V2 UsagePlan
    let mut plan: UsagePlan = match &loaded {
        LoadedPlanAccount::Usage(p) => p.clone(),
        LoadedPlanAccount::LegacyUsage(legacy) => UsagePlan {
            merchant: legacy.merchant,
            plan_id: legacy.plan_id,
            unit_name: legacy.unit_name,
            tiers: legacy.tiers,
            tier_count: legacy.tier_count,
            max_charge_per_period: legacy.max_charge_per_period,
            settlement_frequency: legacy.settlement_frequency,
            credential_mint: legacy.credential_mint,
            billing_mint: Pubkey::default(),
            status: legacy.status.clone(),
            bump: legacy.bump,
            version: crate::state::CURRENT_ACCOUNT_VERSION,
            _reserved: [0; crate::state::ACCOUNT_RESERVED_BYTES - 32],
        },
        _ => return Err(VelaError::BillingTypeMismatch.into()),
    };

    let old_max_charge = plan.max_charge_per_period;
    let old_settlement = plan.settlement_frequency;
    let old_tier_count = plan.tier_count;
    let mut next_tiers_vec: Option<Vec<PricingTier>> = None;
    let next_max_charge = max_charge_per_period.unwrap_or(plan.max_charge_per_period);

    if let Some(new_tiers) = tiers {
        require!(
            !new_tiers.is_empty() && new_tiers.len() <= 5,
            VelaError::InvalidTierCount,
        );

        // Validate tier boundaries
        for i in 1..new_tiers.len() {
            let prev = &new_tiers[i - 1];
            let curr = &new_tiers[i];
            if i < new_tiers.len() - 1 {
                require!(curr.up_to > prev.up_to, VelaError::InvalidTierBoundary);
            } else if curr.up_to != 0 {
                require!(curr.up_to > prev.up_to, VelaError::InvalidTierBoundary);
            }
        }

        validate_usage_pricing_bounds(&new_tiers, next_max_charge)?;

        let mut tiers_array = [PricingTier::default(); 5];
        for (i, tier) in new_tiers.iter().enumerate() {
            tiers_array[i] = *tier;
        }
        plan.tiers = tiers_array;
        plan.tier_count = new_tiers.len() as u8;
        next_tiers_vec = Some(new_tiers);
    }

    if let Some(m) = max_charge_per_period {
        require!(m > 0, VelaError::InvalidAmount);
        plan.max_charge_per_period = m;
    }

    if next_tiers_vec.is_none() {
        let active_tiers = &plan.tiers[..usize::from(plan.tier_count)];
        validate_usage_pricing_bounds(active_tiers, plan.max_charge_per_period)?;
    }

    if let Some(s) = settlement_frequency {
        require!(s > 0, VelaError::InvalidFrequency);
        plan.settlement_frequency = s;
    }

    // Ensure account has enough space for V2
    let required_size = UsagePlan::SIZE;
    if plan_info.data_len() < required_size {
        let rent = Rent::get()?;
        let new_lamports = rent.minimum_balance(required_size);
        let current_lamports = plan_info.lamports();
        if current_lamports < new_lamports {
            anchor_lang::solana_program::program::invoke(
                &anchor_lang::solana_program::system_instruction::transfer(
                    &merchant_key,
                    plan_info.key,
                    new_lamports - current_lamports,
                ),
                &[
                    ctx.accounts.merchant.to_account_info(),
                    plan_info.clone(),
                    ctx.accounts.system_program.to_account_info(),
                ],
            )?;
        }
        plan_info.resize(required_size)?;
    }

    write_usage_plan(&plan_info, &plan)?;

    emit!(UsagePlanUpdated {
        plan: *plan_info.key,
        merchant: merchant_key,
        plan_id: plan.plan_id,
        old_max_charge_per_period: old_max_charge,
        new_max_charge_per_period: plan.max_charge_per_period,
        old_settlement_frequency: old_settlement,
        new_settlement_frequency: plan.settlement_frequency,
        old_tier_count,
        new_tier_count: plan.tier_count,
    });

    Ok(())
}
