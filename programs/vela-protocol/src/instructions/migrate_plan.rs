use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::plan_account::{
        load_plan_account, write_plan, write_usage_plan, LoadedPlanAccount,
    },
    state::{UsagePlan, VelaPlan, CURRENT_ACCOUNT_VERSION, ACCOUNT_RESERVED_BYTES},
};

#[derive(Accounts)]
pub struct MigratePlan<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut)]
    pub plan: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<MigratePlan>) -> Result<()> {
    let plan_info = ctx.accounts.plan.to_account_info();
    let loaded = load_plan_account(&plan_info)?;

    match &loaded {
        LoadedPlanAccount::Flat(plan) => {
            // Already V2 — idempotent, no-op
            if plan.version == CURRENT_ACCOUNT_VERSION {
                return Ok(());
            }
            Err(VelaError::PlanVersionUnsupported.into())
        }
        LoadedPlanAccount::Usage(plan) => {
            // Already V2 — idempotent, no-op
            if plan.version == CURRENT_ACCOUNT_VERSION {
                return Ok(());
            }
            Err(VelaError::PlanVersionUnsupported.into())
        }
        LoadedPlanAccount::LegacyFlat(legacy) => {
            let v2_plan = VelaPlan {
                merchant: legacy.merchant,
                plan_id: legacy.plan_id,
                amount: legacy.amount,
                frequency: legacy.frequency,
                trial_period: legacy.trial_period,
                max_pulls: legacy.max_pulls,
                status: legacy.status.clone(),
                credential_mint: legacy.credential_mint,
                bump: legacy.bump,
                version: CURRENT_ACCOUNT_VERSION,
                _reserved: [0; ACCOUNT_RESERVED_BYTES],
            };

            // Realloc account to V2 size
            let required_size = VelaPlan::SIZE;
            realloc_plan_account(
                &plan_info,
                &ctx.accounts.admin.to_account_info(),
                &ctx.accounts.system_program.to_account_info(),
                &ctx.accounts.rent,
                required_size,
            )?;

            write_plan(&plan_info, &v2_plan)?;
            Ok(())
        }
        LoadedPlanAccount::LegacyUsage(legacy) => {
            let v2_plan = UsagePlan {
                merchant: legacy.merchant,
                plan_id: legacy.plan_id,
                unit_name: legacy.unit_name,
                tiers: legacy.tiers,
                tier_count: legacy.tier_count,
                max_charge_per_period: legacy.max_charge_per_period,
                settlement_frequency: legacy.settlement_frequency,
                credential_mint: legacy.credential_mint,
                status: legacy.status.clone(),
                bump: legacy.bump,
                version: CURRENT_ACCOUNT_VERSION,
                _reserved: [0; ACCOUNT_RESERVED_BYTES],
            };

            // Realloc account to V2 size
            let required_size = UsagePlan::SIZE;
            realloc_plan_account(
                &plan_info,
                &ctx.accounts.admin.to_account_info(),
                &ctx.accounts.system_program.to_account_info(),
                &ctx.accounts.rent,
                required_size,
            )?;

            write_usage_plan(&plan_info, &v2_plan)?;
            Ok(())
        }
    }
}

fn realloc_plan_account<'info>(
    plan_info: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    rent: &Rent,
    required_size: usize,
) -> Result<()> {
    if plan_info.data_len() < required_size {
        let new_lamports = rent.minimum_balance(required_size);
        let current_lamports = plan_info.lamports();
        if current_lamports < new_lamports {
            anchor_lang::solana_program::program::invoke(
                &anchor_lang::solana_program::system_instruction::transfer(
                    payer.key,
                    plan_info.key,
                    new_lamports - current_lamports,
                ),
                &[payer.clone(), plan_info.clone(), system_program.clone()],
            )?;
        }
        plan_info.resize(required_size)?;
    }
    Ok(())
}
