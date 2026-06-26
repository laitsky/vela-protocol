use crate::instructions::arcium_accounts::{
    deserialize_cluster, validate_callback_binding, validate_protocol_config,
    validate_static_callback_accounts,
};
use crate::{
    constants::PULL_APPROVAL_TTL_SECONDS,
    errors::VelaError,
    instructions::{
        arcium_request_state::{complete_arcium_request, validate_pending_arcium_request},
        mandate_account::mandate_billing_period,
        protocol_config_account::load_protocol_config,
    },
    state::{
        ArciumRequestFlow, ArciumRequestState, BillingType, MandateStatus, ProtocolConfig,
        PullApproval, VelaMandate,
    },
    validate_callback_ixs,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, HasSize};

const VALIDATE_MANDATE_CIRCUIT: &str = "validate_mandate_v2";

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ValidateMandateOutput {
    pub field_0: bool,
}

impl ValidateMandateOutput {
    pub const SIZE: usize = 1;
}

impl HasSize for ValidateMandateOutput {
    const SIZE: usize = Self::SIZE;
}

#[arcium_callback(encrypted_ix = "validate_mandate")]
pub fn validate_mandate_callback(
    ctx: Context<ValidateMandateCallback>,
    output: SignedComputationOutputs<ValidateMandateOutput>,
) -> Result<()> {
    let loaded_config = load_protocol_config(&ctx.accounts.config.to_account_info())?;
    let config = loaded_config.into_current();
    validate_protocol_config(&config)?;
    let mxe_program_id = config.effective_mxe_program_id();
    validate_static_callback_accounts(
        &ctx.accounts.mxe_account,
        &ctx.accounts.comp_def_account,
        VALIDATE_MANDATE_CIRCUIT,
        &mxe_program_id,
    )?;
    validate_callback_binding(
        &config,
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
        validate_pending_arcium_request(
            &ctx.accounts.request_state,
            ctx.accounts.mandate.key(),
            ArciumRequestFlow::Validation,
            ctx.accounts.mandate.next_payment_due.to_le_bytes(),
        )?,
    )?;
    let cluster = deserialize_cluster(&ctx.accounts.cluster_account)?;

    let result = match output.verify_output(&cluster, &ctx.accounts.computation_account) {
        Ok(ValidateMandateOutput { field_0 }) => field_0,
        Err(_) => return Err(VelaError::AbortedComputation.into()),
    };

    require!(
        ctx.accounts.mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );
    require!(
        ctx.accounts.mandate.billing_type == BillingType::Flat,
        VelaError::BillingTypeMismatch
    );

    let now = Clock::get()?.unix_timestamp;
    let (period_start, period_end) = mandate_billing_period(&ctx.accounts.mandate)?;
    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = ctx.accounts.mandate.key();
    approval.period_start = period_start;
    approval.period_end = period_end;
    approval.valid_until = now
        .checked_add(PULL_APPROVAL_TTL_SECONDS)
        .ok_or(VelaError::Overflow)?;
    approval.approved = result;
    approval.approved_amount = match ctx.accounts.mandate.billing_type {
        BillingType::Flat => ctx.accounts.mandate.amount,
        BillingType::Usage => {
            // For usage mandates using the flat validation path,
            // this should not happen -- usage mandates use usage_computation_callback.
            // But if called, use mandate.amount as the cap (safe default).
            ctx.accounts.mandate.amount
        }
    };
    approval.created_at = now;
    approval.bump = ctx.bumps.pull_approval;
    complete_arcium_request(&mut ctx.accounts.request_state, now)?;

    emit!(ValidationCompleteEvent {
        mandate: ctx.accounts.mandate.key(),
        approved: result,
        timestamp: now,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct ValidateMandateCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    /// CHECK: Validated against the circuit's comp-def PDA in the handler.
    pub comp_def_account: UncheckedAccount<'info>,
    /// CHECK: Validated against the canonical Arcium MXE PDA in the handler.
    pub mxe_account: UncheckedAccount<'info>,
    /// CHECK: Arcium validates this callback computation account via BLS output verification.
    pub computation_account: UncheckedAccount<'info>,
    /// CHECK: Deserialized manually for BLS output verification.
    pub cluster_account: UncheckedAccount<'info>,
    #[account(address = arcium_anchor::solana_instructions_sysvar::ID)]
    /// CHECK: The address constraint pins this to the instructions sysvar account.
    pub instructions_sysvar: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [
            ArciumRequestState::SEED_PREFIX,
            ArciumRequestFlow::VALIDATION_SEED,
            mandate.key().as_ref(),
            mandate.next_payment_due.to_le_bytes().as_ref(),
        ],
        bump = request_state.bump,
    )]
    pub request_state: Account<'info, ArciumRequestState>,

    #[account(
        mut,
        seeds = [PullApproval::SEED_PREFIX, mandate.key().as_ref()],
        bump,
    )]
    pub pull_approval: Account<'info, PullApproval>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: UncheckedAccount<'info>,

    pub mandate: Account<'info, VelaMandate>,
}

#[event]
pub struct ValidationCompleteEvent {
    pub mandate: Pubkey,
    pub approved: bool,
    pub timestamp: i64,
}
