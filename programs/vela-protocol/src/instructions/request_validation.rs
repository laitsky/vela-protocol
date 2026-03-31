use crate::instructions::arcium_accounts::{
    build_callback_instruction, derive_validation_computation_offset, validate_protocol_config,
    validate_queue_accounts,
};
use crate::{
    errors::VelaError,
    state::{MandateStatus, ProtocolConfig, PullApproval, VelaMandate, VelaPlan},
    ArciumSignerAccount, ID,
};
use anchor_lang::{prelude::*, Discriminator};
use arcium_anchor::{prelude::*, traits::QueueCompAccs};
use arcium_client::idl::arcium::{
    cpi::accounts::QueueComputation as ArciumQueueComputation, types::CallbackAccount,
    ID_CONST,
};

const VALIDATE_MANDATE_CIRCUIT: &str = "validate_mandate";

pub fn request_validation(
    ctx: Context<RequestValidation>,
    requested_computation_offset: u64,
    ciphertext: Vec<[u8; 32]>,
    pub_key: [u8; 32],
    nonce: u128,
) -> Result<()> {
    let mandate = &ctx.accounts.mandate;

    require!(
        mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );

    let clock = Clock::get()?;
    require!(
        clock.unix_timestamp >= mandate.next_payment_due,
        VelaError::PullTooEarly
    );
    require!(
        ctx.accounts.plan.status == crate::state::PlanStatus::Active,
        VelaError::PlanNotActive
    );
    require!(ciphertext.len() == 8, VelaError::InvalidCiphertextInput);
    validate_protocol_config(&ctx.accounts.config)?;

    let next_request_nonce = ctx.accounts.mandate.validation_request_nonce
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    let computation_offset = derive_validation_computation_offset(
        &ctx.accounts.mandate.key(),
        ctx.accounts.mandate.next_payment_due,
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
        VALIDATE_MANDATE_CIRCUIT,
    )?;

    if ctx.accounts.pull_approval.approved
        && clock.unix_timestamp <= ctx.accounts.pull_approval.valid_until
    {
        return Err(VelaError::ApprovalAlreadyExists.into());
    }

    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = Pubkey::default();
    approval.valid_until = 0;
    approval.approved = false;
    approval.created_at = 0;
    approval.bump = ctx.bumps.pull_approval;
    ctx.accounts.mandate.validation_request_nonce = next_request_nonce;

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
            &crate::instruction::ValidateMandateCallback::DISCRIMINATOR,
            computation_offset,
            ctx.accounts.config.cluster_offset,
            VALIDATE_MANDATE_CIRCUIT,
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
            ],
        )?],
        1,
        0,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct RequestValidation<'info> {
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
        space = PullApproval::SIZE,
        seeds = [PullApproval::SEED_PREFIX, mandate.key().as_ref()],
        bump,
    )]
    pub pull_approval: Box<Account<'info, PullApproval>>,

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
