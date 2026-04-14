use crate::instructions::arcium_accounts::{
    build_callback_instruction, derive_billing_computation_offset, validate_protocol_config,
    validate_queue_accounts,
};
use crate::{
    errors::VelaError,
    instructions::protocol_config_account::load_protocol_config,
    state::{BillingEvent, ProtocolConfig, VelaMandate, VelaPlan},
    ArciumSignerAccount, ID,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, traits::QueueCompAccs};
use arcium_client::idl::arcium::{
    cpi::accounts::QueueComputation as ArciumQueueComputation, types::CallbackAccount,
    ID_CONST,
};

const RECORD_BILLING_EVENT_CIRCUIT: &str = "record_billing_event";
const RECORD_BILLING_EVENT_CALLBACK_DISCRIMINATOR: [u8; 8] =
    [123, 164, 16, 122, 130, 206, 223, 55];

pub fn request_billing_record(
    ctx: Context<RequestBillingRecord>,
    requested_computation_offset: u64,
) -> Result<()> {
    require!(ctx.accounts.mandate.pulls_executed > 0, VelaError::ApprovalNotGranted);
    require!(
        ctx.accounts.plan.key() == ctx.accounts.mandate.plan,
        VelaError::PlanNotActive
    );
    require!(
        ctx.accounts.mandate.last_billing_recorded_pull < ctx.accounts.mandate.pulls_executed,
        VelaError::BillingEventAlreadyExists
    );
    let config = load_protocol_config(&ctx.accounts.config.to_account_info())?.into_current();
    validate_protocol_config(&config)?;

    let next_request_nonce = ctx.accounts.mandate.billing_request_nonce
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    let computation_offset = derive_billing_computation_offset(
        &ctx.accounts.mandate.key(),
        ctx.accounts.mandate.pulls_executed,
        next_request_nonce,
    );
    require!(
        requested_computation_offset == computation_offset,
        VelaError::InvalidComputationOffset
    );
    require_keys_eq!(
        ctx.accounts.cluster_account.key(),
        config.cluster_pubkey,
        VelaError::InvalidArciumAccount
    );

    validate_queue_accounts(
        &ctx.accounts.mxe_account,
        &ctx.accounts.mempool_account,
        &ctx.accounts.executing_pool,
        &ctx.accounts.computation_account,
        &ctx.accounts.comp_def_account,
        &ctx.accounts.cluster_account,
        &ctx.accounts.pool_account,
        &ctx.accounts.clock_account,
        config.cluster_offset,
        computation_offset,
        RECORD_BILLING_EVENT_CIRCUIT,
    )?;

    let frequency =
        i64::try_from(ctx.accounts.plan.frequency).map_err(|_| VelaError::Overflow)?;
    let billing_period_end = ctx.accounts.mandate.next_payment_due;
    let billing_period_start = billing_period_end
        .checked_sub(frequency)
        .ok_or(VelaError::Overflow)?;

    if ctx.accounts.billing_event.created_at != 0 {
        return Err(VelaError::BillingEventAlreadyExists.into());
    }

    let billing_event = &mut ctx.accounts.billing_event;
    billing_event.mandate = Pubkey::default();
    billing_event.merchant = Pubkey::default();
    billing_event.subscriber = Pubkey::default();
    billing_event.plan_id = 0;
    billing_event.encrypted_blob = [[0u8; 32]; 10];
    billing_event.nonce = 0;
    billing_event.created_at = 0;
    billing_event.bump = ctx.bumps.billing_event;
    ctx.accounts.mandate.billing_request_nonce = next_request_nonce;

    let args = ArgBuilder::new()
        .plaintext_u64(ctx.accounts.plan.amount)
        .plaintext_i64(ctx.accounts.mandate.last_pull_at)
        .plaintext_u64(ctx.accounts.mandate.pulls_executed)
        .plaintext_i64(billing_period_start)
        .plaintext_i64(billing_period_end)
        .plaintext_u64(0)
        .build();

    ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

    queue_computation(
        &*ctx.accounts,
        computation_offset,
        args,
        vec![build_callback_instruction(
            &RECORD_BILLING_EVENT_CALLBACK_DISCRIMINATOR,
            computation_offset,
            config.cluster_offset,
            RECORD_BILLING_EVENT_CIRCUIT,
            &[
                CallbackAccount {
                    pubkey: ctx.accounts.billing_event.key(),
                    is_writable: true,
                },
                CallbackAccount {
                    pubkey: ctx.accounts.config.key(),
                    is_writable: false,
                },
                CallbackAccount {
                    pubkey: ctx.accounts.mandate.key(),
                    is_writable: false,
                },
                CallbackAccount {
                    pubkey: ctx.accounts.plan.key(),
                    is_writable: false,
                },
            ],
        )?],
        1,
        0,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RequestBillingRecord<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: UncheckedAccount<'info>,

    /// CHECK: Validated against the canonical Arcium MXE PDA in the handler.
    pub mxe_account: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        space = 9,
        payer = payer,
        seeds = [&SIGN_PDA_SEED],
        bump,
        address = derive_sign_pda!(),
    )]
    pub sign_pda_account: Account<'info, ArciumSignerAccount>,

    /// CHECK: Validated against the configured Arcium cluster in the handler.
    #[account(mut)]
    pub mempool_account: UncheckedAccount<'info>,

    /// CHECK: Validated against the configured Arcium cluster in the handler.
    #[account(mut)]
    pub executing_pool: UncheckedAccount<'info>,

    /// CHECK: Validated against the configured Arcium cluster in the handler.
    #[account(mut)]
    pub computation_account: UncheckedAccount<'info>,

    /// CHECK: Validated against the circuit's comp-def PDA in the handler.
    pub comp_def_account: UncheckedAccount<'info>,

    /// CHECK: Validated against the configured Arcium cluster in the handler.
    #[account(mut)]
    pub cluster_account: UncheckedAccount<'info>,

    /// CHECK: Validated against the Arcium fee-pool PDA in the handler.
    #[account(mut)]
    pub pool_account: UncheckedAccount<'info>,

    /// CHECK: Validated against the Arcium clock PDA in the handler.
    #[account(mut)]
    pub clock_account: UncheckedAccount<'info>,

    #[account(
        seeds = [
            VelaPlan::SEED_PREFIX,
            plan.merchant.as_ref(),
            plan.plan_id.to_le_bytes().as_ref()
        ],
        bump = plan.bump
    )]
    pub plan: Box<Account<'info, VelaPlan>>,

    #[account(
        mut,
        seeds = [
            VelaMandate::SEED_PREFIX,
            mandate.subscriber.as_ref(),
            plan.key().as_ref()
        ],
        bump = mandate.bump
    )]
    pub mandate: Box<Account<'info, VelaMandate>>,

    #[account(
        init_if_needed,
        payer = payer,
        space = BillingEvent::SIZE,
        seeds = [
            BillingEvent::SEED_PREFIX,
            mandate.key().as_ref(),
            mandate.pulls_executed.to_le_bytes().as_ref(),
        ],
        bump,
    )]
    pub billing_event: Box<Account<'info, BillingEvent>>,

    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
}

impl<'info> QueueCompAccs<'info> for RequestBillingRecord<'info> {
    fn comp_def_offset(&self) -> u32 {
        comp_def_offset(RECORD_BILLING_EVENT_CIRCUIT)
    }

    fn queue_comp_accs(&self) -> ArciumQueueComputation<'info> {
        ArciumQueueComputation {
            signer: self.payer.to_account_info(),
            sign_seed: self.sign_pda_account.to_account_info(),
            comp: self.computation_account.to_account_info(),
            mxe: self.mxe_account.to_account_info(),
            mempool: self.mempool_account.to_account_info(),
            executing_pool: self.executing_pool.to_account_info(),
            comp_def_acc: self.comp_def_account.to_account_info(),
            cluster: self.cluster_account.to_account_info(),
            pool_account: self.pool_account.to_account_info(),
            system_program: self.system_program.to_account_info(),
            clock: self.clock_account.to_account_info(),
        }
    }

    fn arcium_program(&self) -> AccountInfo<'info> {
        self.arcium_program.to_account_info()
    }

    fn mxe_program(&self) -> Pubkey {
        ID
    }

    fn signer_pda_bump(&self) -> u8 {
        self.sign_pda_account.bump
    }
}
