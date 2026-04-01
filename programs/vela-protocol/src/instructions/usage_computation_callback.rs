use crate::instructions::arcium_accounts::{
    derive_usage_computation_offset, deserialize_cluster, validate_callback_binding,
    validate_protocol_config, validate_static_callback_accounts,
};
use crate::{
    errors::VelaError,
    state::{ProtocolConfig, PullApproval, VelaMandate, UsageReport},
    validate_callback_ixs,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, HasSize};

const USAGE_CHARGE_CIRCUIT: &str = "usage_charge";

/// Output struct for both usage_charge and tiered_pricing circuits.
/// Both circuits return a single u64 plaintext value (the computed charge amount).
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UsageChargeOutput {
    pub field_0: u64,
}

impl UsageChargeOutput {
    pub const SIZE: usize = 8;
}

impl HasSize for UsageChargeOutput {
    const SIZE: usize = Self::SIZE;
}

/// Arcium callback for usage charge computation.
/// Handles both usage_charge (1-tier) and tiered_pricing (multi-tier) circuit outputs
/// since both return the same u64 type.
/// Function is named usage_charge_callback per arcium_callback macro naming requirement:
/// the function name must be <encrypted_ix_name>_callback.
#[arcium_callback(encrypted_ix = "usage_charge")]
pub fn usage_charge_callback(
    ctx: Context<UsageComputationCallback>,
    output: SignedComputationOutputs<UsageChargeOutput>,
) -> Result<()> {
    validate_static_callback_accounts(
        &ctx.accounts.mxe_account,
        &ctx.accounts.comp_def_account,
        USAGE_CHARGE_CIRCUIT,
    )?;
    validate_protocol_config(&ctx.accounts.config)?;
    validate_callback_binding(
        &ctx.accounts.config,
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
        derive_usage_computation_offset(
            &ctx.accounts.mandate.key(),
            ctx.accounts.usage_report.period_start,
            ctx.accounts.mandate.validation_request_nonce,
        ),
    )?;
    let cluster = deserialize_cluster(&ctx.accounts.cluster_account)?;

    let computed_charge = match output.verify_output(&cluster, &ctx.accounts.computation_account) {
        Ok(UsageChargeOutput { field_0 }) => field_0,
        Err(_) => return Err(VelaError::AbortedComputation.into()),
    };

    // Write PullApproval with the Arcium-computed charge amount (not mandate.amount)
    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = ctx.accounts.mandate.key();
    approval.valid_until = ctx.accounts.mandate.next_payment_due;
    approval.approved = true; // successful usage computation means approval
    approval.approved_amount = computed_charge;
    approval.created_at = Clock::get()?.unix_timestamp;
    approval.bump = ctx.bumps.pull_approval;

    // Mark usage report as settled
    ctx.accounts.usage_report.settled = true;

    emit!(UsageComputationCompleteEvent {
        mandate: ctx.accounts.mandate.key(),
        approved_amount: computed_charge,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct UsageComputationCallback<'info> {
    pub arcium_program: Program<'info, Arcium>,
    /// CHECK: Validated against the circuit's comp-def PDA in the handler.
    pub comp_def_account: UncheckedAccount<'info>,
    /// CHECK: Validated against the canonical Arcium MXE PDA in the handler.
    pub mxe_account: UncheckedAccount<'info>,
    /// CHECK: Arcium validates this callback computation account via BLS output verification.
    pub computation_account: UncheckedAccount<'info>,
    /// CHECK: Deserialized manually for BLS output verification.
    pub cluster_account: UncheckedAccount<'info>,
    #[account(address = ::anchor_lang::solana_program::sysvar::instructions::ID)]
    /// CHECK: The address constraint pins this to the instructions sysvar account.
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [PullApproval::SEED_PREFIX, mandate.key().as_ref()],
        bump,
    )]
    pub pull_approval: Account<'info, PullApproval>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = config.bump,
    )]
    pub config: Account<'info, ProtocolConfig>,

    pub mandate: Account<'info, VelaMandate>,

    #[account(mut)]
    pub usage_report: Account<'info, UsageReport>,
}

#[event]
pub struct UsageComputationCompleteEvent {
    pub mandate: Pubkey,
    pub approved_amount: u64,
    pub timestamp: i64,
}
