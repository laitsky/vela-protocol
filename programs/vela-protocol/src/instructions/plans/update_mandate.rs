use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction},
};

use crate::{
    errors::VelaError,
    instructions::{
        mandate_account::{
            load_mandate_account, validate_loaded_mandate_address, write_mandate_account,
        },
        plan_account::load_plan_account,
    },
    state::{VelaMandate, ACCOUNT_RESERVED_BYTES},
};

#[derive(Accounts)]
pub struct UpdateMandate<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    /// CHECK: Read and validated manually to support both flat/usage plans.
    pub plan: UncheckedAccount<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<UpdateMandate>,
    amount: Option<u64>,
    frequency: Option<u64>,
    max_pulls: Option<u64>,
    billing_type: Option<crate::state::BillingType>,
    plan: Option<Pubkey>,
) -> Result<()> {
    require!(
        amount.is_some()
            || frequency.is_some()
            || max_pulls.is_some()
            || billing_type.is_some()
            || plan.is_some(),
        VelaError::NoUpdateProvided
    );

    let merchant_key = ctx.accounts.merchant.key();
    let mandate = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &mandate)?;
    require_keys_eq!(
        mandate.merchant(),
        merchant_key,
        VelaError::UnauthorizedCancel
    );
    require!(
        matches!(mandate.status(), crate::state::MandateStatus::Active),
        VelaError::MandateNotActive
    );

    let target_plan_key = plan.unwrap_or(mandate.plan());
    require_keys_eq!(
        ctx.accounts.plan.key(),
        target_plan_key,
        VelaError::MigrationPreconditionFailed
    );
    let target_plan = load_plan_account(&ctx.accounts.plan.to_account_info())?;
    require_keys_eq!(
        target_plan.merchant(),
        merchant_key,
        VelaError::UnauthorizedCancel
    );
    require!(
        matches!(target_plan.status(), crate::state::PlanStatus::Active),
        VelaError::PlanNotActive
    );

    // Explicit same-address realloc path for V2 mandates (MND-05).
    ensure_min_mandate_size(
        &ctx.accounts.merchant.to_account_info(),
        &ctx.accounts.mandate.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
    )?;

    // Active economic mutation is intentionally blocked here. This instruction remains as a
    // compatibility/realloc path; plan, price, cadence, cap, and billing-type changes for active
    // mandates must use update_mandate_plan so subscriber approval and proration rules apply.
    if let Some(next_amount) = amount {
        require!(
            next_amount == mandate.amount(),
            VelaError::UnauthorizedUpgrade
        );
    }
    if let Some(next_frequency) = frequency {
        require!(
            next_frequency == mandate.frequency(),
            VelaError::UnauthorizedUpgrade
        );
    }
    if let Some(next_max_pulls) = max_pulls {
        require!(
            next_max_pulls == mandate.max_pulls(),
            VelaError::UnauthorizedUpgrade
        );
    }
    if plan.is_some() {
        require_keys_eq!(
            target_plan_key,
            mandate.plan(),
            VelaError::UnauthorizedUpgrade
        );
    }

    if let Some(next_billing_type) = billing_type {
        require!(
            next_billing_type == mandate.billing_type(),
            VelaError::UnauthorizedUpgrade
        );
        require!(
            next_billing_type == target_plan.billing_type(),
            VelaError::BillingTypeMismatch
        );
    }

    write_mandate_account(&ctx.accounts.mandate.to_account_info(), &mandate)?;
    Ok(())
}

fn ensure_min_mandate_size<'info>(
    payer: &AccountInfo<'info>,
    mandate_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<()> {
    if mandate_info.data_len() >= VelaMandate::SIZE {
        return Ok(());
    }

    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(VelaMandate::SIZE);
    let current_lamports = mandate_info.lamports();
    if current_lamports < required_lamports {
        invoke(
            &system_instruction::transfer(
                payer.key,
                mandate_info.key,
                required_lamports - current_lamports,
            ),
            &[payer.clone(), mandate_info.clone(), system_program.clone()],
        )?;
    }

    mandate_info.resize(VelaMandate::SIZE)?;
    let mut data = mandate_info.try_borrow_mut_data()?;
    if data.len() >= VelaMandate::SIZE {
        let reserved_start = VelaMandate::SIZE - ACCOUNT_RESERVED_BYTES;
        for byte in &mut data[reserved_start..VelaMandate::SIZE] {
            *byte = 0;
        }
    }
    Ok(())
}
