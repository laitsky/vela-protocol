use crate::instructions::arcium_accounts::{
    derive_validation_computation_offset, deserialize_cluster, validate_callback_binding,
    validate_protocol_config, validate_static_callback_accounts,
};
use crate::{
    errors::VelaError,
    state::{ProtocolConfig, PullApproval, VelaMandate},
    validate_callback_ixs,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, HasSize};

const VALIDATE_MANDATE_CIRCUIT: &str = "validate_mandate";

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
    validate_static_callback_accounts(
        &ctx.accounts.mxe_account,
        &ctx.accounts.comp_def_account,
        VALIDATE_MANDATE_CIRCUIT,
    )?;
    validate_protocol_config(&ctx.accounts.config)?;
    validate_callback_binding(
        &ctx.accounts.config,
        &ctx.accounts.cluster_account,
        &ctx.accounts.computation_account,
        derive_validation_computation_offset(
            &ctx.accounts.mandate.key(),
            ctx.accounts.mandate.next_payment_due,
            ctx.accounts.mandate.validation_request_nonce,
        ),
    )?;
    let cluster = deserialize_cluster(&ctx.accounts.cluster_account)?;

    let result = match output.verify_output(&cluster, &ctx.accounts.computation_account) {
        Ok(ValidateMandateOutput { field_0 }) => field_0,
        Err(_) => return Err(VelaError::AbortedComputation.into()),
    };

    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = ctx.accounts.mandate.key();
    approval.valid_until = ctx.accounts.mandate.next_payment_due;
    approval.approved = result;
    approval.approved_amount = ctx.accounts.mandate.amount;
    approval.created_at = Clock::get()?.unix_timestamp;
    approval.bump = ctx.bumps.pull_approval;

    emit!(ValidationCompleteEvent {
        mandate: ctx.accounts.mandate.key(),
        approved: result,
        timestamp: Clock::get()?.unix_timestamp,
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
}

#[event]
pub struct ValidationCompleteEvent {
    pub mandate: Pubkey,
    pub approved: bool,
    pub timestamp: i64,
}
