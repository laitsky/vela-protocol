use crate::instructions::arcium_accounts::{
    build_callback_instruction, derive_validation_computation_offset, validate_protocol_config,
    validate_queue_accounts,
};
use crate::{
    errors::VelaError,
    instructions::{
        arcium_request_state::{start_arcium_request, StartArciumRequestArgs},
        mandate_account::{load_mandate_account, validate_loaded_mandate_address, write_mandate},
        plan_account::{load_plan_account, require_plan_billing_type},
        protocol_config_account::load_protocol_config,
    },
    state::{
        ArciumRequestFlow, ArciumRequestState, BillingType, MandateStatus, ProtocolConfig,
        PullApproval,
    },
    ArciumSignerAccount, ID,
};
use anchor_lang::prelude::*;
use arcium_anchor::{prelude::*, traits::QueueCompAccs};
use arcium_client::idl::arcium::{
    cpi::accounts::QueueComputation as ArciumQueueComputation, types::CallbackAccount, ID_CONST,
};

const VALIDATE_MANDATE_CIRCUIT: &str = "validate_mandate";
const VALIDATE_MANDATE_CALLBACK_DISCRIMINATOR: [u8; 8] = [18, 21, 173, 122, 11, 126, 79, 200];

pub fn request_validation(
    ctx: Context<RequestValidation>,
    requested_computation_offset: u64,
    next_payment_due_seed: i64,
    ciphertext: Vec<[u8; 32]>,
    pub_key: [u8; 32],
    nonce: u128,
) -> Result<()> {
    let loaded_mandate = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &loaded_mandate)?;
    let legacy_layout = loaded_mandate.is_legacy();
    let mut mandate = loaded_mandate.into_current();
    let plan = load_plan_account(&ctx.accounts.plan.to_account_info())?;

    require_keys_eq!(
        ctx.accounts.plan.key(),
        mandate.plan,
        VelaError::PlanNotActive
    );
    require_plan_billing_type(&plan, &BillingType::Flat)?;

    require!(
        mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );
    require!(
        mandate.amount == plan.mandate_amount(),
        VelaError::PlanNotActive
    );
    require!(
        mandate.pulls_executed < mandate.max_pulls,
        VelaError::MaxPullsExceeded
    );

    let clock = Clock::get()?;
    if mandate.expiry > 0 {
        require!(
            clock.unix_timestamp < mandate.expiry,
            VelaError::MandateExpired
        );
    }
    require!(
        clock.unix_timestamp >= mandate.next_payment_due,
        VelaError::PullTooEarly
    );
    require!(
        next_payment_due_seed == mandate.next_payment_due,
        VelaError::InvalidArciumRequestState
    );
    require!(
        *plan.status() == crate::state::PlanStatus::Active,
        VelaError::PlanNotActive
    );
    require!(ciphertext.len() == 8, VelaError::InvalidCiphertextInput);
    let config = load_protocol_config(&ctx.accounts.config.to_account_info())?.into_current();
    validate_protocol_config(&config)?;

    let next_request_nonce = mandate
        .validation_request_nonce
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    let computation_offset = derive_validation_computation_offset(
        &ctx.accounts.mandate.key(),
        mandate.next_payment_due,
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
        VALIDATE_MANDATE_CIRCUIT,
    )?;

    if ctx.accounts.pull_approval.approved
        && clock.unix_timestamp <= ctx.accounts.pull_approval.valid_until
    {
        return Err(VelaError::ApprovalAlreadyExists.into());
    }

    start_arcium_request(
        &mut ctx.accounts.request_state,
        StartArciumRequestArgs {
            mandate: ctx.accounts.mandate.key(),
            flow: ArciumRequestFlow::Validation,
            subject: next_payment_due_seed.to_le_bytes(),
            computation_offset,
            request_nonce: next_request_nonce,
            bump: ctx.bumps.request_state,
            now: clock.unix_timestamp,
        },
    )?;

    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = Pubkey::default();
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

    let args = ArgBuilder::new()
        .x25519_pubkey(pub_key)
        .plaintext_u128(nonce)
        .encrypted_u64(ciphertext[0])
        .encrypted_u64(ciphertext[1])
        .encrypted_u64(ciphertext[2])
        .encrypted_i64(ciphertext[3])
        .encrypted_i64(ciphertext[4])
        .encrypted_i64(ciphertext[5])
        .encrypted_u64(ciphertext[6])
        .encrypted_u64(ciphertext[7])
        .build();

    ctx.accounts.sign_pda_account.bump = ctx.bumps.sign_pda_account;

    queue_computation(
        &*ctx.accounts,
        computation_offset,
        args,
        vec![build_callback_instruction(
            &VALIDATE_MANDATE_CALLBACK_DISCRIMINATOR,
            computation_offset,
            config.cluster_offset,
            VALIDATE_MANDATE_CIRCUIT,
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
                    pubkey: ctx.accounts.mandate.key(),
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
#[instruction(requested_computation_offset: u64, next_payment_due_seed: i64)]
pub struct RequestValidation<'info> {
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

    /// CHECK: Deserialized and validated manually to support current and legacy flat plans.
    pub plan: UncheckedAccount<'info>,

    /// CHECK: Deserialized and validated manually to support current and legacy mandate layouts.
    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

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
            ArciumRequestFlow::VALIDATION_SEED,
            mandate.key().as_ref(),
            next_payment_due_seed.to_le_bytes().as_ref(),
        ],
        bump,
    )]
    pub request_state: Box<Account<'info, ArciumRequestState>>,

    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
}

impl<'info> QueueCompAccs<'info> for RequestValidation<'info> {
    fn comp_def_offset(&self) -> u32 {
        comp_def_offset(VALIDATE_MANDATE_CIRCUIT)
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
