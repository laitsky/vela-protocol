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
        ArciumRequestFlow, ArciumRequestState, BillingEvent, ProtocolConfig, VelaMandate, VelaPlan,
    },
    validate_callback_ixs,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, HasSize, MXEEncryptedStruct};

const RECORD_BILLING_EVENT_CIRCUIT: &str = "record_billing_event";

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct RecordBillingEventOutput {
    pub field_0: MXEEncryptedStruct<10>,
}

impl RecordBillingEventOutput {
    pub const SIZE: usize = 16 + (10 * 32);
}

impl HasSize for RecordBillingEventOutput {
    const SIZE: usize = Self::SIZE;
}

#[arcium_callback(encrypted_ix = "record_billing_event")]
pub fn record_billing_event_callback(
    ctx: Context<RecordBillingEventCallback>,
    output: SignedComputationOutputs<RecordBillingEventOutput>,
) -> Result<()> {
    validate_static_callback_accounts(
        &ctx.accounts.mxe_account,
        &ctx.accounts.comp_def_account,
        RECORD_BILLING_EVENT_CIRCUIT,
    )?;
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
            ArciumRequestFlow::BillingRecord,
            ctx.accounts.mandate.pulls_executed.to_le_bytes(),
        )?,
    )?;
    let cluster = deserialize_cluster(&ctx.accounts.cluster_account)?;

    let encrypted_data = match output.verify_output(&cluster, &ctx.accounts.computation_account) {
        Ok(RecordBillingEventOutput { field_0 }) => field_0,
        Err(_) => return Err(VelaError::AbortedComputation.into()),
    };

    require_keys_eq!(
        ctx.accounts.plan.key(),
        ctx.accounts.mandate.plan,
        VelaError::PlanNotActive
    );

    let mandate = &mut ctx.accounts.mandate;
    let billing_event = &mut ctx.accounts.billing_event;
    require!(
        billing_event.created_at == 0,
        VelaError::BillingEventAlreadyExists
    );
    billing_event.mandate = mandate.key();
    billing_event.merchant = mandate.merchant;
    billing_event.subscriber = mandate.subscriber;
    billing_event.plan_id = ctx.accounts.plan.plan_id;
    billing_event.encrypted_blob = encrypted_data.ciphertexts;
    billing_event.nonce = encrypted_data.nonce;
    let now = Clock::get()?.unix_timestamp;
    billing_event.created_at = now;
    billing_event.bump = ctx.bumps.billing_event;
    mandate.last_billing_recorded_pull = mandate.pulls_executed;
    complete_arcium_request(&mut ctx.accounts.request_state, now)?;

    emit!(BillingEventCreatedEvent {
        mandate: mandate.key(),
        merchant: mandate.merchant,
        subscriber: mandate.subscriber,
        pulls_executed: mandate.pulls_executed,
        timestamp: now,
    });

    Ok(())
}

#[derive(Accounts)]
pub struct RecordBillingEventCallback<'info> {
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
            ArciumRequestFlow::BILLING_RECORD_SEED,
            mandate.key().as_ref(),
            mandate.pulls_executed.to_le_bytes().as_ref(),
        ],
        bump = request_state.bump,
    )]
    pub request_state: Account<'info, ArciumRequestState>,

    #[account(
        mut,
        seeds = [
            BillingEvent::SEED_PREFIX,
            mandate.key().as_ref(),
            mandate.pulls_executed.to_le_bytes().as_ref(),
        ],
        bump,
    )]
    pub billing_event: Account<'info, BillingEvent>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: UncheckedAccount<'info>,

    #[account(mut)]
    pub mandate: Account<'info, VelaMandate>,
    pub plan: Account<'info, VelaPlan>,
}

#[event]
pub struct BillingEventCreatedEvent {
    pub mandate: Pubkey,
    pub merchant: Pubkey,
    pub subscriber: Pubkey,
    pub pulls_executed: u64,
    pub timestamp: i64,
}
