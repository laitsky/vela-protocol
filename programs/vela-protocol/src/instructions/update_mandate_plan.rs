use anchor_lang::prelude::*;
use anchor_spl::token_2022::Token2022;

use crate::{
    constants::{EXTRA_ACCOUNT_METAS_SEED, WRAPPED_USDC_SYMBOL},
    errors::VelaError,
    instructions::{
        execute_stream::invoke_stream_transfer,
        mandate_account::{load_mandate_account, validate_loaded_mandate_address, write_mandate},
        plan_account::load_plan_account,
        protocol_config_account::load_protocol_config,
        compute_proration,
    },
    state::{
        MandateCreditAdded, MandateStatus, MandateUpgradeFinalized, MandateUpgradeInitiated,
        ProtocolConfig, PullApproval, TokenConfig, VelaMandate,
    },
};

#[derive(Accounts)]
pub struct UpdateMandatePlan<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Deserialized and validated manually to support flat and usage plans.
    pub new_plan: UncheckedAccount<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

    /// CHECK: Source wrapped token account validated by the downstream transfer + hook path.
    #[account(mut)]
    pub subscriber_wrapped_account: UncheckedAccount<'info>,

    /// CHECK: Destination wrapped token account validated by the downstream transfer + hook path.
    #[account(mut)]
    pub merchant_wrapped_account: UncheckedAccount<'info>,

    /// CHECK: Wrapped mint validated against protocol config.
    #[account(mut)]
    pub wrapped_usdc_mint: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Pull approval PDA validated manually when a positive proration requires a charge.
    pub pull_approval: UncheckedAccount<'info>,

    /// CHECK: TokenConfig PDA validated against the wrapped mint.
    pub token_config: UncheckedAccount<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    /// CHECK: Wrapping vault validated against protocol config.
    #[account(mut)]
    pub wrapping_vault: UncheckedAccount<'info>,

    /// CHECK: Transfer hook program validated against protocol config.
    pub hook_program: UncheckedAccount<'info>,

    #[account(
        seeds = [EXTRA_ACCOUNT_METAS_SEED, wrapped_usdc_mint.key().as_ref()],
        bump,
        seeds::program = hook_program.key(),
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// CHECK: Main protocol executable, required as an external-PDA derivation program for the hook.
    #[account(address = crate::ID)]
    pub protocol_program: UncheckedAccount<'info>,

    pub token_2022_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<UpdateMandatePlan>) -> Result<()> {
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require!(!protocol_config.paused(), VelaError::ProtocolPaused);
    require!(
        protocol_config.transfer_hook_program_id() != Pubkey::default(),
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        ctx.accounts.hook_program.key(),
        protocol_config.transfer_hook_program_id(),
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.key(),
        protocol_config.wrapping_vault(),
        VelaError::VaultMismatch
    );
    require_keys_eq!(
        ctx.accounts.wrapped_usdc_mint.key(),
        protocol_config.wrapped_usdc_mint(),
        VelaError::TokenChangeNotSupported
    );

    let (expected_token_config, _) = Pubkey::find_program_address(
        &[TokenConfig::SEED_PREFIX, ctx.accounts.wrapped_usdc_mint.key().as_ref()],
        &crate::ID,
    );
    require_keys_eq!(
        ctx.accounts.token_config.key(),
        expected_token_config,
        VelaError::TokenChangeNotSupported
    );

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

    let new_amount = new_plan.mandate_amount();
    if new_amount > mandate.amount {
        require!(is_subscriber, VelaError::UnauthorizedUpgrade);
    }

    let new_plan_key = ctx.accounts.new_plan.key();
    if mandate.plan == new_plan_key {
        return Ok(());
    }

    let clock_now = Clock::get()?.unix_timestamp;
    let elapsed_seconds = elapsed_in_period(&mandate, clock_now)?;
    let signed_delta = compute_proration(mandate.amount, new_amount, elapsed_seconds, mandate.frequency)?;
    let event_proration_amount = i64::try_from(signed_delta).map_err(|_| error!(VelaError::MathOverflow))?;

    let old_plan = mandate.plan;
    emit!(MandateUpgradeInitiated {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        mint: protocol_config.wrapped_usdc_mint(),
        token_symbol: WRAPPED_USDC_SYMBOL.to_string(),
        old_plan,
        new_plan: new_plan_key,
        proration_amount: event_proration_amount,
        change_type: 1,
        signer: authority,
        applied_at: clock_now,
        timestamp: clock_now,
    });

    if signed_delta > 0 {
        let charge_amount = u64::try_from(signed_delta).map_err(|_| error!(VelaError::MathOverflow))?;
        validate_pull_approval(&ctx, charge_amount, clock_now)?;

        let mandate_index_bytes = mandate.mandate_index.to_le_bytes();
        let mandate_bump = [mandate.bump];
        let signer_seeds: &[&[u8]] = &[
            VelaMandate::SEED_PREFIX,
            mandate.subscriber.as_ref(),
            mandate.merchant.as_ref(),
            mandate_index_bytes.as_ref(),
            &mandate_bump,
        ];
        invoke_stream_transfer(
            &ctx.accounts.subscriber_wrapped_account.to_account_info(),
            &ctx.accounts.wrapped_usdc_mint.to_account_info(),
            &ctx.accounts.merchant_wrapped_account.to_account_info(),
            &ctx.accounts.mandate.to_account_info(),
            &ctx.accounts.protocol_program.to_account_info(),
            &ctx.accounts.wrapping_vault.to_account_info(),
            &ctx.accounts.protocol_config.to_account_info(),
            &ctx.accounts.pull_approval.to_account_info(),
            &ctx.accounts.token_config.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
            &ctx.accounts.extra_account_meta_list.to_account_info(),
            &ctx.accounts.hook_program.to_account_info(),
            &ctx.accounts.token_2022_program.to_account_info(),
            charge_amount,
            &[signer_seeds],
        )?;
    } else if signed_delta < 0 {
        let credit_amount =
            u64::try_from(-signed_delta).map_err(|_| error!(VelaError::MathOverflow))?;
        mandate.credit_balance = mandate
            .credit_balance
            .checked_add(credit_amount)
            .ok_or(VelaError::Overflow)?;
        emit!(MandateCreditAdded {
            schema_version: 1,
            mandate: ctx.accounts.mandate.key(),
            mint: protocol_config.wrapped_usdc_mint(),
            old_plan,
            new_plan: new_plan_key,
            credit_amount,
            new_credit_balance: mandate.credit_balance,
            applied_at: clock_now,
            timestamp: clock_now,
        });
    }

    mandate.plan = new_plan_key;
    mandate.amount = new_plan.mandate_amount();
    mandate.frequency = new_plan.mandate_frequency();
    mandate.max_pulls = new_plan.max_pulls();
    mandate.billing_type = new_plan.billing_type();
    mandate.version = crate::state::CURRENT_ACCOUNT_VERSION;
    mandate.clear_pending();
    write_mandate(&ctx.accounts.mandate.to_account_info(), &mandate, false)?;

    emit!(MandateUpgradeFinalized {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        mint: protocol_config.wrapped_usdc_mint(),
        token_symbol: WRAPPED_USDC_SYMBOL.to_string(),
        old_plan,
        new_plan: new_plan_key,
        proration_amount: event_proration_amount,
        change_type: 1,
        applied_at: clock_now,
        timestamp: clock_now,
    });

    Ok(())
}

fn elapsed_in_period(mandate: &VelaMandate, clock_now: i64) -> Result<u64> {
    let period_total_seconds = mandate.frequency;
    let period_total_i64 = i64::try_from(period_total_seconds).map_err(|_| VelaError::Overflow)?;
    let period_start = mandate
        .next_payment_due
        .checked_sub(period_total_i64)
        .ok_or(VelaError::Overflow)?;

    if clock_now <= period_start {
        return Ok(0);
    }
    if clock_now >= mandate.next_payment_due {
        return Ok(period_total_seconds);
    }

    u64::try_from(clock_now - period_start).map_err(|_| VelaError::Overflow.into())
}

fn validate_pull_approval(
    ctx: &Context<UpdateMandatePlan>,
    charge_amount: u64,
    clock_now: i64,
) -> Result<()> {
    let (expected_approval, _) = Pubkey::find_program_address(
        &[PullApproval::SEED_PREFIX, ctx.accounts.mandate.key().as_ref()],
        &crate::ID,
    );
    require_keys_eq!(
        ctx.accounts.pull_approval.key(),
        expected_approval,
        VelaError::ApprovalNotGranted
    );

    if ctx.accounts.pull_approval.owner != &crate::ID || ctx.accounts.pull_approval.data_is_empty() {
        return Err(VelaError::ApprovalNotGranted.into());
    }

    let approval = {
        let approval_data = ctx.accounts.pull_approval.try_borrow_data()?;
        let mut approval_slice: &[u8] = &approval_data;
        PullApproval::try_deserialize(&mut approval_slice)
            .map_err(|_| VelaError::ApprovalNotGranted)?
    };

    require!(approval.approved, VelaError::ApprovalNotGranted);
    require!(clock_now <= approval.valid_until, VelaError::ApprovalExpired);
    require!(
        charge_amount <= approval.approved_amount,
        VelaError::AmountExceedsPlanAmount
    );

    Ok(())
}
