use crate::instructions::arcium_accounts::{
    deserialize_cluster, validate_callback_binding, validate_protocol_config,
    validate_static_callback_accounts,
};
use crate::{
    errors::VelaError,
    instructions::{
        arcium_request_state::{complete_arcium_request, validate_pending_arcium_request},
        protocol_config_account::load_protocol_config,
    },
    state::{
        ArciumRequestFlow, ArciumRequestState, ProtocolConfig, PullApproval, UsageReport,
        VelaMandate,
    },
    validate_callback_ixs,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, HasSize};

const USAGE_CHARGE_CIRCUIT: &str = "usage_charge";
const TIERED_PRICING_CIRCUIT: &str = "tiered_pricing";

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
    validate_usage_callback_accounts(&ctx.accounts.mxe_account, &ctx.accounts.comp_def_account)?;
    let loaded_config = load_protocol_config(&ctx.accounts.config.to_account_info())?;
    let config = loaded_config.into_current();
    validate_protocol_config(&config)?;
    validate_callback_binding(
        &config,
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
        validate_pending_arcium_request(
            &ctx.accounts.request_state,
            ctx.accounts.mandate.key(),
            ArciumRequestFlow::UsageComputation,
            ctx.accounts.usage_report.period_start.to_le_bytes(),
        )?,
    )?;
    let cluster = deserialize_cluster(&ctx.accounts.cluster_account)?;

    let computed_charge = match output.verify_output(&cluster, &ctx.accounts.computation_account) {
        Ok(UsageChargeOutput { field_0 }) => field_0,
        Err(_) => return Err(VelaError::AbortedComputation.into()),
    };

    require_keys_eq!(
        ctx.accounts.usage_report.mandate,
        ctx.accounts.mandate.key(),
        VelaError::BillingTypeMismatch
    );
    require!(
        !ctx.accounts.usage_report.settled,
        VelaError::UsageReportAlreadySettled
    );
    require!(
        computed_charge <= ctx.accounts.mandate.amount,
        VelaError::AmountExceedsPlanAmount
    );

    // Write PullApproval with the Arcium-computed charge amount (not mandate.amount)
    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = ctx.accounts.mandate.key();
    approval.valid_until = ctx.accounts.mandate.next_payment_due;
    approval.approved = true; // successful usage computation means approval
    approval.approved_amount = computed_charge;
    let now = Clock::get()?.unix_timestamp;
    approval.created_at = now;
    approval.bump = ctx.bumps.pull_approval;
    complete_arcium_request(&mut ctx.accounts.request_state, now)?;

    // Mark usage report as settled
    ctx.accounts.usage_report.settled = true;

    emit!(UsageComputationCompleteEvent {
        mandate: ctx.accounts.mandate.key(),
        approved_amount: computed_charge,
        timestamp: now,
    });

    Ok(())
}

fn validate_usage_callback_accounts(
    mxe_account: &UncheckedAccount<'_>,
    comp_def_account: &UncheckedAccount<'_>,
) -> Result<()> {
    validate_static_callback_accounts(mxe_account, comp_def_account, USAGE_CHARGE_CIRCUIT).or_else(
        |_| {
            validate_static_callback_accounts(mxe_account, comp_def_account, TIERED_PRICING_CIRCUIT)
        },
    )
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
        seeds = [
            ArciumRequestState::SEED_PREFIX,
            ArciumRequestFlow::USAGE_COMPUTATION_SEED,
            mandate.key().as_ref(),
            usage_report.period_start.to_le_bytes().as_ref(),
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

    #[account(
        mut,
        seeds = [
            UsageReport::SEED_PREFIX,
            mandate.key().as_ref(),
            usage_report.period_start.to_le_bytes().as_ref(),
        ],
        bump = usage_report.bump,
    )]
    pub usage_report: Account<'info, UsageReport>,
}

#[event]
pub struct UsageComputationCompleteEvent {
    pub mandate: Pubkey,
    pub approved_amount: u64,
    pub timestamp: i64,
}
