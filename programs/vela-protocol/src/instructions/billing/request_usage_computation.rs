use crate::instructions::arcium_accounts::{
    build_callback_instruction, derive_usage_computation_offset, validate_protocol_config,
    validate_queue_accounts,
};
use crate::{
    errors::VelaError,
    instructions::{
        arcium_request_state::{start_arcium_request, StartArciumRequestArgs},
        create_usage_plan::validate_usage_pricing_bounds,
        mandate_account::{
            load_mandate_account, mandate_billing_period, validate_loaded_mandate_address,
            write_mandate,
        },
        plan_account::usage_plan_terms_hash_from_parts,
        protocol_config_account::load_protocol_config,
    },
    state::{
        ArciumRequestFlow, ArciumRequestState, BillingType, MandateStatus, ProtocolConfig,
        PullApproval, UsagePlan, UsageReport,
    },
    ArciumSignerAccount, ID,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, traits::QueueCompAccs};
use arcium_client::idl::arcium::{
    cpi::accounts::QueueComputation as ArciumQueueComputation, types::CallbackAccount,
};

const USAGE_CHARGE_CIRCUIT: &str = "usage_charge_v2";
const TIERED_PRICING_CIRCUIT: &str = "tiered_pricing_v2";
const USAGE_COMPUTATION_CALLBACK_DISCRIMINATOR: [u8; 8] = [201, 76, 6, 25, 189, 59, 96, 63];
const USAGE_UNITS_CIPHERTEXT_COUNT: u8 = 1;

pub fn request_usage_computation(
    ctx: Context<RequestUsageComputation>,
    requested_computation_offset: u64,
) -> Result<()> {
    let loaded = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &loaded)?;
    let legacy_layout = loaded.is_legacy();
    let mut mandate = loaded.into_current();
    let usage_plan = &ctx.accounts.usage_plan;
    let usage_report = &ctx.accounts.usage_report;

    // Only usage mandates can queue usage computations
    require!(
        mandate.billing_type == BillingType::Usage,
        VelaError::BillingTypeMismatch
    );
    require_keys_eq!(
        ctx.accounts.usage_plan.key(),
        mandate.plan,
        VelaError::PlanNotActive
    );
    require!(
        mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );
    // Prevent double-settlement of the same usage report
    require!(!usage_report.settled, VelaError::UsageReportAlreadySettled);
    // Ensure the report belongs to this mandate
    require_keys_eq!(
        usage_report.mandate,
        ctx.accounts.mandate.key(),
        VelaError::BillingTypeMismatch
    );
    require_keys_eq!(
        usage_report.merchant,
        mandate.merchant,
        VelaError::BillingTypeMismatch
    );
    let (period_start, period_end) = mandate_billing_period(&mandate)?;
    require!(
        usage_report.period_start == period_start && usage_report.period_end == period_end,
        VelaError::PeriodMismatch
    );
    let clock = Clock::get()?;
    require!(clock.unix_timestamp >= period_end, VelaError::PullTooEarly);

    let config = load_protocol_config(&ctx.accounts.config.to_account_info())?.into_current();
    validate_protocol_config(&config)?;
    let mxe_program_id = config.effective_mxe_program_id();

    let mandate_key = ctx.accounts.mandate.key();
    let next_request_nonce = mandate
        .validation_request_nonce
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    let computation_offset = derive_usage_computation_offset(
        &mandate_key,
        usage_report.period_start,
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

    require!(
        usage_report.ciphertext_count == USAGE_UNITS_CIPHERTEXT_COUNT,
        VelaError::InvalidCiphertextInput
    );
    require!(
        usage_plan.tier_count > 0 && usage_plan.tier_count <= 5,
        VelaError::InvalidTierCount
    );
    validate_usage_pricing_bounds(
        &usage_plan.tiers[..usize::from(usage_plan.tier_count)],
        usage_plan.max_charge_per_period,
    )?;
    let expected_terms_hash = usage_plan_terms_hash_from_parts(
        &ctx.accounts.usage_plan.key(),
        &usage_plan.merchant,
        usage_plan.plan_id,
        &usage_plan.tiers,
        usage_plan.tier_count,
        usage_plan.max_charge_per_period,
        usage_plan.settlement_frequency,
    );
    require!(
        usage_report.computation_ciphertext[1] == expected_terms_hash,
        VelaError::InvalidCiphertextInput
    );

    // Select circuit based on tier_count: 1 tier = simple usage_charge, >1 = tiered_pricing.
    // The merchant report supplies only encrypted usage units; pricing terms come from the
    // canonical on-chain UsagePlan snapshot verified above.
    let circuit_name = if usage_plan.tier_count == 1 {
        USAGE_CHARGE_CIRCUIT
    } else {
        TIERED_PRICING_CIRCUIT
    };

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
        circuit_name,
        &mxe_program_id,
    )?;

    if ctx.accounts.pull_approval.approved
        && ctx.accounts.pull_approval.mandate == mandate_key
        && ctx.accounts.pull_approval.period_start == period_start
        && ctx.accounts.pull_approval.period_end == period_end
        && clock.unix_timestamp <= ctx.accounts.pull_approval.valid_until
    {
        return Err(VelaError::ApprovalAlreadyExists.into());
    }

    start_arcium_request(
        &mut ctx.accounts.request_state,
        StartArciumRequestArgs {
            mandate: mandate_key,
            flow: ArciumRequestFlow::UsageComputation,
            subject: usage_report.period_start.to_le_bytes(),
            computation_offset,
            request_nonce: next_request_nonce,
            bump: ctx.bumps.request_state,
            now: clock.unix_timestamp,
        },
    )?;

    // Reset pull_approval state while computation is in-flight
    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = Pubkey::default();
    approval.period_start = 0;
    approval.period_end = 0;
    approval.valid_until = 0;
    approval.approved = false;
    approval.approved_amount = 0;
    approval.created_at = 0;
    approval.bump = ctx.bumps.pull_approval;

    mandate.validation_request_nonce = next_request_nonce;
    write_mandate(
        &ctx.accounts.mandate.to_account_info(),
        &mandate,
        legacy_layout,
    )?;

    // Queue only encrypted usage units from the merchant report; all pricing terms are plaintext
    // on-chain UsagePlan values committed by the terms hash stored with the report.
    let mut args = ArgBuilder::new()
        .x25519_pubkey(usage_report.pub_key)
        .plaintext_u128(usage_report.nonce)
        .encrypted_u64(usage_report.computation_ciphertext[0]);
    if usage_plan.tier_count == 1 {
        args = args
            .plaintext_u64(usage_plan.tiers[0].rate_per_unit)
            .plaintext_u64(usage_plan.max_charge_per_period);
    } else {
        for tier in usage_plan.tiers.iter() {
            args = args.plaintext_u64(tier.up_to);
        }
        for tier in usage_plan.tiers.iter() {
            args = args.plaintext_u64(tier.rate_per_unit);
        }
        args = args
            .plaintext_u8(usage_plan.tier_count)
            .plaintext_u64(usage_plan.max_charge_per_period);
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
            config.cluster_offset,
            circuit_name,
            &mxe_program_id,
            &[
                CallbackAccount {
                    pubkey: ctx.accounts.request_state.key(),
                    is_writable: true,
                },
                CallbackAccount {
                    pubkey: ctx.accounts.pull_approval.key(),
                    is_writable: true,
                },
                CallbackAccount {
                    pubkey: ctx.accounts.config.key(),
                    is_writable: false,
                },
                CallbackAccount {
                    pubkey: mandate_key,
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

    /// CHECK: Deserialized and validated manually to support both legacy and V2 mandate layouts.
    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

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

    #[account(
        init_if_needed,
        payer = payer,
        space = ArciumRequestState::SIZE,
        seeds = [
            ArciumRequestState::SEED_PREFIX,
            ArciumRequestFlow::USAGE_COMPUTATION_SEED,
            mandate.key().as_ref(),
            usage_report.period_start.to_le_bytes().as_ref(),
        ],
        bump,
    )]
    pub request_state: Box<Account<'info, ArciumRequestState>>,

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
        load_protocol_config(&self.config.to_account_info())
            .map(|config| config.into_current().effective_mxe_program_id())
            .unwrap_or(ID)
    }

    fn signer_pda_bump(&self) -> u8 {
        self.sign_pda_account.bump
    }
}
