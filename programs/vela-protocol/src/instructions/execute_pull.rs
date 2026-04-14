use anchor_lang::{
    prelude::*,
    solana_program::program::invoke_signed,
};
use anchor_spl::token_2022::Token2022;
use solana_instruction::{AccountMeta as SplAccountMeta, Instruction as SplInstruction};
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;

use crate::{
    constants::{
        EXTRA_ACCOUNT_METAS_SEED, TRANSFER_HOOK_PROGRAM_ID, USDC_DECIMALS,
    },
    errors::VelaError,
    instructions::{
        keeper_config_account::load_keeper_config,
        plan_account::{load_plan_account, require_plan_billing_type, LoadedPlanAccount},
        protocol_config_account::load_protocol_config,
    },
    state::{BillingType, KeeperConfig, MandateStatus, PlanStatus, ProtocolConfig, PullApproval, VelaMandate},
};

#[derive(Accounts)]
pub struct ExecutePull<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Used for mandate PDA derivation only.
    pub subscriber: UncheckedAccount<'info>,

    /// CHECK: Used for plan PDA validation only.
    pub merchant: UncheckedAccount<'info>,

    #[account(
        seeds = [KeeperConfig::SEED_PREFIX],
        bump,
    )]
    pub keeper_config: UncheckedAccount<'info>,

    /// CHECK: Deserialized and validated manually to support both flat and usage plans.
    pub plan: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [
            VelaMandate::SEED_PREFIX,
            subscriber.key().as_ref(),
            plan.key().as_ref()
        ],
        bump = mandate.bump
    )]
    pub mandate: Box<Account<'info, VelaMandate>>,

    /// Subscriber's Token-2022 wrapped USDC account (source of the transfer).
    /// CHECK: Token account mint and authority validated in handler.
    #[account(mut)]
    pub subscriber_wrapped_account: UncheckedAccount<'info>,

    /// Merchant's Token-2022 wrapped USDC account (destination of the transfer).
    /// CHECK: Token account mint and authority validated in handler.
    #[account(mut)]
    pub merchant_wrapped_account: UncheckedAccount<'info>,

    /// The Token-2022 wrapped USDC mint charged by the billing transfer.
    /// CHECK: Validated in handler against loaded protocol_config.wrapped_usdc_mint.
    #[account(mut)]
    pub wrapped_usdc_mint: UncheckedAccount<'info>,

    /// CHECK: The handler validates the PDA derivation, ownership, and deserializes PullApproval manually.
    #[account(mut)]
    pub pull_approval: UncheckedAccount<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    /// CHECK: Wrapping vault validated in handler against loaded protocol_config.wrapping_vault.
    #[account(mut)]
    pub wrapping_vault: UncheckedAccount<'info>,

    /// CHECK: Dedicated transfer-hook validator program.
    #[account(
        address = TRANSFER_HOOK_PROGRAM_ID,
    )]
    pub hook_program: UncheckedAccount<'info>,

    /// CHECK: PDA owned by the hook program and derived from the wrapped mint.
    #[account(
        seeds = [EXTRA_ACCOUNT_METAS_SEED, wrapped_usdc_mint.key().as_ref()],
        bump,
        seeds::program = hook_program.key(),
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// CHECK: Main protocol executable, required as an external-PDA derivation program for the hook.
    #[account(address = crate::ID)]
    pub protocol_program: UncheckedAccount<'info>,

    pub token_2022_program: Program<'info, Token2022>,
}

pub fn handler<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ExecutePull<'info>>,
) -> Result<()> {
    let keeper_config = load_keeper_config(&ctx.accounts.keeper_config.to_account_info())?;
    require_keys_eq!(
        ctx.accounts.payer.key(),
        keeper_config.keeper_authority(),
        VelaError::UnauthorizedKeeper
    );
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require!(
        !protocol_config.paused(),
        VelaError::ProtocolPaused
    );
    require_keys_eq!(
        ctx.accounts.wrapped_usdc_mint.key(),
        protocol_config.wrapped_usdc_mint(),
        VelaError::UsdcMintMismatch
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.key(),
        protocol_config.wrapping_vault(),
        VelaError::VaultMismatch
    );
    let plan = load_plan_account(&ctx.accounts.plan.to_account_info())?;
    require_plan_billing_type(&plan, &ctx.accounts.mandate.billing_type)?;
    require!(*plan.status() == PlanStatus::Active, VelaError::PlanNotActive);
    require!(
        ctx.accounts.mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );
    require_keys_eq!(ctx.accounts.plan.key(), ctx.accounts.mandate.plan);
    require_keys_eq!(plan.merchant(), ctx.accounts.merchant.key());
    require!(
        ctx.accounts.mandate.last_billing_recorded_pull == ctx.accounts.mandate.pulls_executed,
        VelaError::PendingBillingRecord
    );

    let clock = Clock::get()?;
    if ctx.accounts.mandate.expiry > 0 {
        require!(
            clock.unix_timestamp < ctx.accounts.mandate.expiry,
            VelaError::MandateExpired
        );
    }
    require!(
        ctx.accounts.mandate.pulls_executed < ctx.accounts.mandate.max_pulls,
        VelaError::MaxPullsExceeded
    );
    require!(
        clock.unix_timestamp >= ctx.accounts.mandate.next_payment_due,
        VelaError::PullTooEarly
    );
    require_eq!(
        ctx.accounts.mandate.amount,
        plan.mandate_amount(),
        VelaError::AmountExceedsPlanAmount
    );
    require_eq!(
        ctx.accounts.mandate.frequency,
        plan.mandate_frequency(),
        VelaError::InvalidFrequency
    );

    // Validate PullApproval PDA derivation and existence.
    // The actual approval validation (approved, valid_until, approved_amount) happens in the
    // transfer hook when Token-2022 fires the Execute CPI during transfer_checked below.
    let (expected_approval, _) = Pubkey::find_program_address(
        &[PullApproval::SEED_PREFIX, ctx.accounts.mandate.key().as_ref()],
        &crate::ID,
    );
    require_keys_eq!(
        ctx.accounts.pull_approval.key(),
        expected_approval,
        VelaError::ApprovalNotGranted
    );

    if ctx.accounts.pull_approval.owner != &crate::ID || ctx.accounts.pull_approval.data_is_empty() {
        emit!(ArciumUnavailableEvent {
            mandate: ctx.accounts.mandate.key(),
            timestamp: clock.unix_timestamp,
        });
        return Err(VelaError::ApprovalNotGranted.into());
    }

    // Pre-validate the approval before invoking Token-2022 transfer_checked (belt-and-suspenders).
    // The dedicated transfer-hook program re-validates the same approval during the actual billing
    // transfer, but doing a local check here gives cleaner user-facing errors and avoids wasting CU.
    let approval = {
        let approval_data = ctx.accounts.pull_approval.try_borrow_data()?;
        let mut approval_slice: &[u8] = &approval_data;
        PullApproval::try_deserialize(&mut approval_slice)
            .map_err(|_| VelaError::ApprovalNotGranted)?
    };

    require!(approval.approved, VelaError::ApprovalNotGranted);
    require!(
        clock.unix_timestamp <= approval.valid_until,
        VelaError::ApprovalExpired
    );
    let charge_amount = match &plan {
        LoadedPlanAccount::Flat(plan) => {
            require!(
                plan.amount <= approval.approved_amount,
                VelaError::AmountExceedsPlanAmount
            );
            plan.amount
        }
        LoadedPlanAccount::Usage(_) => {
            require!(
                approval.approved_amount <= ctx.accounts.mandate.amount,
                VelaError::AmountExceedsPlanAmount
            );
            approval.approved_amount
        }
    };

    // Settle the billing move as an actual Token-2022 transfer_checked.
    // The wrapped mint points at the dedicated `vela-transfer-hook` program, so the hook fires
    // during this CPI without re-entering the main protocol program.
    let subscriber_key = ctx.accounts.subscriber.key();
    let plan_key = ctx.accounts.plan.key();
    let mandate_bump = [ctx.accounts.mandate.bump];
    let mandate_signer_seeds: &[&[u8]] = &[
        VelaMandate::SEED_PREFIX,
        subscriber_key.as_ref(),
        plan_key.as_ref(),
        &mandate_bump,
    ];
    let mandate_signer_seed_groups = [mandate_signer_seeds];
    let source_info = ctx.accounts.subscriber_wrapped_account.to_account_info();
    let mint_info = ctx.accounts.wrapped_usdc_mint.to_account_info();
    let destination_info = ctx.accounts.merchant_wrapped_account.to_account_info();
    let authority_info = ctx.accounts.mandate.to_account_info();
    let mut transfer_ix = spl_token_2022::instruction::transfer_checked(
        &spl_pubkey(ctx.accounts.token_2022_program.key),
        &spl_pubkey(source_info.key),
        &spl_pubkey(mint_info.key),
        &spl_pubkey(destination_info.key),
        &spl_pubkey(authority_info.key),
        &[],
        charge_amount,
        USDC_DECIMALS,
    )
    .map_err(map_spl_error)?;
    transfer_ix.accounts.extend_from_slice(&[
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.protocol_program.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.wrapping_vault.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.protocol_config.key()), false),
        SplAccountMeta::new(spl_pubkey(&ctx.accounts.pull_approval.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.extra_account_meta_list.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.hook_program.key()), false),
    ]);
    let transfer_ix = convert_instruction(transfer_ix);
    let transfer_account_infos = [
        source_info.clone(),
        mint_info.clone(),
        destination_info.clone(),
        authority_info.clone(),
        ctx.accounts.protocol_program.to_account_info(),
        ctx.accounts.wrapping_vault.to_account_info(),
        ctx.accounts.protocol_config.to_account_info(),
        ctx.accounts.pull_approval.to_account_info(),
        ctx.accounts.extra_account_meta_list.to_account_info(),
        ctx.accounts.hook_program.to_account_info(),
    ];
    invoke_signed(
        &transfer_ix,
        &transfer_account_infos,
        &mandate_signer_seed_groups,
    )?;

    let mandate_frequency =
        i64::try_from(ctx.accounts.mandate.frequency).map_err(|_| VelaError::Overflow)?;
    let is_usage_mandate = ctx.accounts.mandate.billing_type == BillingType::Usage;
    let mandate = &mut ctx.accounts.mandate;
    mandate.pulls_executed = mandate
        .pulls_executed
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    mandate.next_payment_due = mandate
        .next_payment_due
        .checked_add(mandate_frequency)
        .ok_or(VelaError::Overflow)?;
    mandate.last_pull_at = clock.unix_timestamp;
    if is_usage_mandate {
        mandate.last_billing_recorded_pull = mandate.pulls_executed;
    }
    if mandate.pulls_executed >= mandate.max_pulls {
        mandate.status = MandateStatus::Expired;
    }

    // Close PullApproval PDA and refund lamports to payer.
    let approval_info = ctx.accounts.pull_approval.to_account_info();
    let payer_info = ctx.accounts.payer.to_account_info();
    let refund = approval_info.lamports();
    **payer_info.lamports.borrow_mut() = payer_info
        .lamports()
        .checked_add(refund)
        .ok_or(VelaError::Overflow)?;
    **approval_info.lamports.borrow_mut() = 0;

    Ok(())
}

#[event]
pub struct ArciumUnavailableEvent {
    pub mandate: Pubkey,
    pub timestamp: i64,
}

fn spl_pubkey(key: &Pubkey) -> SplPubkey {
    SplPubkey::from(key.to_bytes())
}

fn anchor_pubkey(key: SplPubkey) -> Pubkey {
    Pubkey::new_from_array(key.to_bytes())
}

fn convert_instruction(ix: SplInstruction) -> anchor_lang::solana_program::instruction::Instruction {
    anchor_lang::solana_program::instruction::Instruction {
        program_id: anchor_pubkey(ix.program_id),
        accounts: ix
            .accounts
            .into_iter()
            .map(|meta| {
                if meta.is_writable {
                    anchor_lang::solana_program::instruction::AccountMeta::new(
                        anchor_pubkey(meta.pubkey),
                        meta.is_signer,
                    )
                } else {
                    anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
                        anchor_pubkey(meta.pubkey),
                        meta.is_signer,
                    )
                }
            })
            .collect(),
        data: ix.data,
    }
}

fn map_spl_error(_error: SplProgramError) -> anchor_lang::error::Error {
    anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData)
}
