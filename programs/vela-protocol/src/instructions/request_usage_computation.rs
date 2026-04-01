use crate::instructions::arcium_accounts::{
    build_callback_instruction, derive_usage_computation_offset, validate_protocol_config,
    validate_queue_accounts,
};
use crate::{
    errors::VelaError,
    state::{BillingType, MandateStatus, ProtocolConfig, PullApproval, VelaMandate, UsagePlan, UsageReport},
    ArciumSignerAccount, ID,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, traits::QueueCompAccs};
use arcium_client::idl::arcium::{
    cpi::accounts::QueueComputation as ArciumQueueComputation, types::CallbackAccount,
    ID_CONST,
};

const USAGE_CHARGE_CIRCUIT: &str = "usage_charge";
const TIERED_PRICING_CIRCUIT: &str = "tiered_pricing";
const USAGE_COMPUTATION_CALLBACK_DISCRIMINATOR: [u8; 8] =
    [201, 76, 6, 25, 189, 59, 96, 63];

pub fn request_usage_computation(
    ctx: Context<RequestUsageComputation>,
    requested_computation_offset: u64,
    ciphertext: Vec<[u8; 32]>,
    pub_key: [u8; 32],
    nonce: u128,
) -> Result<()> {
    let mandate = &ctx.accounts.mandate;
    let usage_plan = &ctx.accounts.usage_plan;
    let usage_report = &ctx.accounts.usage_report;

    // Only usage mandates can queue usage computations
    require!(
        mandate.billing_type == BillingType::Usage,
        VelaError::BillingTypeMismatch
    );
    require!(
        mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );
    // Prevent double-settlement of the same usage report
    require!(
        !usage_report.settled,
        VelaError::UsageReportAlreadySettled
    );
    // Ensure the report belongs to this mandate
    require_keys_eq!(
        usage_report.mandate,
        mandate.key(),
        VelaError::BillingTypeMismatch
    );

    validate_protocol_config(&ctx.accounts.config)?;

    let next_request_nonce = mandate
        .validation_request_nonce
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    let computation_offset = derive_usage_computation_offset(
        &mandate.key(),
        usage_report.period_start,
        next_request_nonce,
    );
    require!(
        requested_computation_offset == computation_offset,
        VelaError::InvalidComputationOffset
    );
    require_keys_eq!(
        ctx.accounts.cluster_account.key(),
        ctx.accounts.config.cluster_pubkey,
        VelaError::InvalidArciumAccount
    );

    // Select circuit based on tier_count: 1 tier = simple usage_charge, >1 = tiered_pricing
    let circuit_name = if usage_plan.tier_count == 1 {
        USAGE_CHARGE_CIRCUIT
    } else {
        TIERED_PRICING_CIRCUIT
    };

    let expected_ciphertext_len = if usage_plan.tier_count == 1 { 3 } else { 13 };
    require!(
        ciphertext.len() == expected_ciphertext_len,
        VelaError::InvalidCiphertextInput
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
        ctx.accounts.config.cluster_offset,
        computation_offset,
        circuit_name,
    )?;

    let clock = Clock::get()?;
    if ctx.accounts.pull_approval.approved
        && clock.unix_timestamp <= ctx.accounts.pull_approval.valid_until
    {
        return Err(VelaError::ApprovalAlreadyExists.into());
    }

    // Reset pull_approval state while computation is in-flight
    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = Pubkey::default();
    approval.valid_until = 0;
    approval.approved = false;
    approval.approved_amount = 0;
    approval.created_at = 0;
    approval.bump = ctx.bumps.pull_approval;

    ctx.accounts.mandate.validation_request_nonce = next_request_nonce;

    // Build ArgBuilder for the selected circuit.
    // usage_charge circuit expects: (usage_units: Enc<Shared,u64>, rate_per_unit: Enc<Shared,u64>, max_charge: Enc<Shared,u64>)
    // tiered_pricing circuit expects: (usage_units: Enc<Shared,u64>, tier_boundaries: Enc<Shared,[u64;5]>, tier_rates: Enc<Shared,[u64;5]>, tier_count: Enc<Shared,u8>, max_charge: Enc<Shared,u64>)
    // In both cases the full encrypted payload is passed via ciphertext parameter from the client.
    let mut args = ArgBuilder::new().x25519_pubkey(pub_key).plaintext_u128(nonce);
    for ct in &ciphertext {
        args = args.encrypted_u64(*ct);
    }
    let args = args.build();

    ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

    queue_computation(
        &*ctx.accounts,
        computation_offset,
        args,
        vec![build_callback_instruction(
            &USAGE_COMPUTATION_CALLBACK_DISCRIMINATOR,
            computation_offset,
            ctx.accounts.config.cluster_offset,
            circuit_name,
            &[
                CallbackAccount {
                    pubkey: ctx.accounts.pull_approval.key(),
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
                    pubkey: ctx.accounts.usage_report.key(),
                    is_writable: true,
                },
            ],
        )?],
        1,
        0,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RequestUsageComputation<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = config.bump,
    )]
    pub config: Box<Account<'info, ProtocolConfig>>,

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
            UsagePlan::SEED_PREFIX,
            usage_plan.merchant.as_ref(),
            usage_plan.plan_id.to_le_bytes().as_ref(),
        ],
        bump = usage_plan.bump,
    )]
    pub usage_plan: Box<Account<'info, UsagePlan>>,

    #[account(
        mut,
        seeds = [
            VelaMandate::SEED_PREFIX,
            mandate.subscriber.as_ref(),
            mandate.plan.as_ref()
        ],
        bump = mandate.bump
    )]
    pub mandate: Box<Account<'info, VelaMandate>>,

    #[account(
        mut,
        seeds = [
            UsageReport::SEED_PREFIX,
            mandate.key().as_ref(),
            usage_report.period_start.to_le_bytes().as_ref(),
        ],
        bump = usage_report.bump,
    )]
    pub usage_report: Box<Account<'info, UsageReport>>,

    #[account(
        init_if_needed,
        payer = payer,
        space = PullApproval::SIZE,
        seeds = [PullApproval::SEED_PREFIX, mandate.key().as_ref()],
        bump,
    )]
    pub pull_approval: Box<Account<'info, PullApproval>>,

    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
}

impl<'info> QueueCompAccs<'info> for RequestUsageComputation<'info> {
    fn comp_def_offset(&self) -> u32 {
        // Select circuit offset dynamically based on tier_count
        let circuit_name = if self.usage_plan.tier_count == 1 {
            USAGE_CHARGE_CIRCUIT
        } else {
            TIERED_PRICING_CIRCUIT
        };
        comp_def_offset(circuit_name)
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
