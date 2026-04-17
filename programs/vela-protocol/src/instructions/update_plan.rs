use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::plan_account::{load_plan_account, write_plan, LoadedPlanAccount},
    state::VelaPlan,
};

#[event]
pub struct PlanUpdated {
    pub plan: Pubkey,
    pub merchant: Pubkey,
    pub plan_id: u64,
    pub old_amount: u64,
    pub new_amount: u64,
    pub old_frequency: u64,
    pub new_frequency: u64,
    pub old_trial_period: u64,
    pub new_trial_period: u64,
    pub old_max_pulls: u64,
    pub new_max_pulls: u64,
}

#[derive(Accounts)]
pub struct UpdatePlan<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(mut)]
    pub plan: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<UpdatePlan>,
    amount: Option<u64>,
    frequency: Option<u64>,
    trial_period: Option<u64>,
    max_pulls: Option<u64>,
) -> Result<()> {
    require!(
        amount.is_some() || frequency.is_some() || trial_period.is_some() || max_pulls.is_some(),
        VelaError::NoUpdateProvided,
    );

    let plan_info = ctx.accounts.plan.to_account_info();
    let merchant_key = ctx.accounts.merchant.key();

    let loaded = load_plan_account(&plan_info)?;

    // Must be a flat plan (V1 or V2)
    require!(loaded.is_flat(), VelaError::BillingTypeMismatch);

    // Must be the plan owner
    require!(
        loaded.merchant() == merchant_key,
        VelaError::UnauthorizedCancel,
    );

    // If V2, we can directly mutate; if V1, migrate first then mutate
    let mut plan: VelaPlan = match &loaded {
        LoadedPlanAccount::Flat(p) => p.clone(),
        LoadedPlanAccount::LegacyFlat(legacy) => VelaPlan {
            merchant: legacy.merchant,
            plan_id: legacy.plan_id,
            amount: legacy.amount,
            frequency: legacy.frequency,
            trial_period: legacy.trial_period,
            max_pulls: legacy.max_pulls,
            status: legacy.status.clone(),
            credential_mint: legacy.credential_mint,
            bump: legacy.bump,
            version: crate::state::CURRENT_ACCOUNT_VERSION,
            _reserved: [0; crate::state::ACCOUNT_RESERVED_BYTES],
        },
        _ => return Err(VelaError::BillingTypeMismatch.into()),
    };

    let old_amount = plan.amount;
    let old_frequency = plan.frequency;
    let old_trial_period = plan.trial_period;
    let old_max_pulls = plan.max_pulls;

    if let Some(a) = amount {
        plan.amount = a;
    }
    if let Some(f) = frequency {
        plan.frequency = f;
    }
    if let Some(t) = trial_period {
        plan.trial_period = t;
    }
    if let Some(m) = max_pulls {
        plan.max_pulls = m;
    }

    // Ensure account has enough space for V2
    let required_size = VelaPlan::SIZE;
    if plan_info.data_len() < required_size {
        // Realloc to V2 size
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

    write_plan(&plan_info, &plan)?;

    emit!(PlanUpdated {
        plan: *plan_info.key,
        merchant: merchant_key,
        plan_id: plan.plan_id,
        old_amount,
        new_amount: plan.amount,
        old_frequency,
        new_frequency: plan.frequency,
        old_trial_period,
        new_trial_period: plan.trial_period,
        old_max_pulls,
        new_max_pulls: plan.max_pulls,
    });

    Ok(())
}
