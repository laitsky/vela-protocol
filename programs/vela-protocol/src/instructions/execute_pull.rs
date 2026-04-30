use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::token_2022::Token2022;
use solana_instruction::{AccountMeta as SplAccountMeta, Instruction as SplInstruction};
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;

use crate::{
    constants::{event_token_symbol, EXTRA_ACCOUNT_METAS_SEED},
    errors::VelaError,
    instructions::{
        account_close::close_program_account,
        keeper_config_account::load_keeper_config,
        mandate_account::{load_mandate_account, validate_loaded_mandate_address, write_mandate},
        plan_account::{load_plan_account, require_plan_billing_type, LoadedPlanAccount},
        protocol_config_account::load_protocol_config,
    },
    state::{
        BillingType, KeeperConfig, MandateStatus, MandateUpgradeFinalized, PlanStatus,
        ProtocolConfig, PullApproval, TokenConfig, VelaMandate,
    },
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

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

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
        seeds = [TokenConfig::SEED_PREFIX, wrapped_usdc_mint.key().as_ref()],
        bump = token_config.bump,
    )]
    pub token_config: Account<'info, TokenConfig>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    /// CHECK: Wrapping vault validated in handler against loaded protocol_config.wrapping_vault.
    #[account(mut)]
    pub wrapping_vault: UncheckedAccount<'info>,

    /// CHECK: Transfer hook program validated in handler against ProtocolConfig.transfer_hook_program_id.
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
    pub system_program: Program<'info, System>,
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
    require!(!protocol_config.paused(), VelaError::ProtocolPaused);
    let hook_program_id = protocol_config.transfer_hook_program_id();
    require!(
        hook_program_id != Pubkey::default(),
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        ctx.accounts.hook_program.key(),
        hook_program_id,
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.key(),
        protocol_config.wrapping_vault(),
        VelaError::VaultMismatch
    );
    let current_plan = load_plan_account(&ctx.accounts.plan.to_account_info())?;
    let loaded_mandate = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &loaded_mandate)?;
    let legacy_layout = loaded_mandate.is_legacy();
    let mut mandate = loaded_mandate.into_current();
    require_plan_billing_type(&current_plan, &mandate.billing_type)?;
    require!(
        *current_plan.status() == PlanStatus::Active,
        VelaError::PlanNotActive
    );
    let billing_mint = current_plan.billing_mint();
    if billing_mint != Pubkey::default() {
        require_keys_eq!(
            ctx.accounts.wrapped_usdc_mint.key(),
            billing_mint,
            VelaError::TokenChangeNotSupported
        );
    } else {
        require_keys_eq!(
            ctx.accounts.wrapped_usdc_mint.key(),
            protocol_config.wrapped_usdc_mint(),
            VelaError::UsdcMintMismatch
        );
    }
    require!(ctx.accounts.token_config.enabled, VelaError::TokenDisabled);
    require_keys_eq!(
        ctx.accounts.token_config.mint,
        ctx.accounts.wrapped_usdc_mint.key(),
        VelaError::TokenNotRegistered
    );
    require!(
        matches!(mandate.status, MandateStatus::Active),
        VelaError::MandateNotActive
    );
    require_keys_eq!(ctx.accounts.plan.key(), mandate.plan);
    require_keys_eq!(current_plan.merchant(), ctx.accounts.merchant.key());
    require_keys_eq!(mandate.subscriber, ctx.accounts.subscriber.key());
    require_keys_eq!(mandate.merchant, ctx.accounts.merchant.key());
    require!(
        mandate.last_billing_recorded_pull == mandate.pulls_executed,
        VelaError::PendingBillingRecord
    );

    let clock = Clock::get()?;
    if mandate.expiry > 0 {
        require!(
            clock.unix_timestamp < mandate.expiry,
            VelaError::MandateExpired
        );
    }
    require!(
        mandate.pulls_executed < mandate.max_pulls,
        VelaError::MaxPullsExceeded
    );
    require!(
        clock.unix_timestamp >= mandate.next_payment_due,
        VelaError::PullTooEarly
    );

    let mut active_plan = current_plan;
    let mut auto_applied_change: Option<(Pubkey, Pubkey, i64)> = None;
    if mandate.pending_change_type == 2 && clock.unix_timestamp >= mandate.pending_effective_at {
        let pending_plan_info = ctx
            .remaining_accounts
            .first()
            .ok_or(VelaError::PendingPlanAccountMissing)?;
        require_keys_eq!(
            pending_plan_info.key(),
            mandate.pending_new_plan,
            VelaError::PendingPlanAccountMissing
        );
        let pending_plan = load_plan_account(pending_plan_info)?;
        require_keys_eq!(
            pending_plan.merchant(),
            mandate.merchant,
            VelaError::UnauthorizedUpgrade
        );
        require!(
            *pending_plan.status() == PlanStatus::Active,
            VelaError::PlanNotActive
        );
        require_plan_billing_type(&pending_plan, &mandate.billing_type)?;
        let active_billing_mint = active_plan.billing_mint();
        let pending_billing_mint = pending_plan.billing_mint();
        if active_billing_mint != Pubkey::default() && pending_billing_mint != Pubkey::default() {
            require_keys_eq!(
                pending_billing_mint,
                active_billing_mint,
                VelaError::TokenChangeNotSupported
            );
        }

        let old_plan = mandate.plan;
        let new_plan_key = mandate.pending_new_plan;
        mandate.plan = new_plan_key;
        mandate.amount = pending_plan.mandate_amount();
        mandate.frequency = pending_plan.mandate_frequency();
        mandate.max_pulls = pending_plan.max_pulls();
        mandate.billing_type = pending_plan.billing_type();
        mandate.version = crate::state::CURRENT_ACCOUNT_VERSION;
        mandate.clear_pending();
        active_plan = pending_plan;
        auto_applied_change = Some((old_plan, new_plan_key, clock.unix_timestamp));
    }

    // Validate PullApproval PDA derivation and existence.
    // The actual approval validation (approved, valid_until, approved_amount) happens in the
    // transfer hook when Token-2022 fires the Execute CPI during transfer_checked below.
    let (expected_approval, _) = Pubkey::find_program_address(
        &[
            PullApproval::SEED_PREFIX,
            ctx.accounts.mandate.key().as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(
        ctx.accounts.pull_approval.key(),
        expected_approval,
        VelaError::ApprovalNotGranted
    );

    if ctx.accounts.pull_approval.owner != &crate::ID || ctx.accounts.pull_approval.data_is_empty()
    {
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
    let base_charge_amount = match &active_plan {
        LoadedPlanAccount::Flat(_) | LoadedPlanAccount::LegacyFlat(_) => {
            require!(
                mandate.amount <= approval.approved_amount,
                VelaError::AmountExceedsPlanAmount
            );
            mandate.amount
        }
        LoadedPlanAccount::Usage(_) | LoadedPlanAccount::LegacyUsage(_) => {
            require!(
                approval.approved_amount <= mandate.amount,
                VelaError::AmountExceedsPlanAmount
            );
            approval.approved_amount
        }
    };
    let credit_used = base_charge_amount.min(mandate.credit_balance);
    let charge_amount = base_charge_amount.saturating_sub(credit_used);
    mandate.credit_balance = mandate.credit_balance.saturating_sub(credit_used);

    // Settle the billing move as an actual Token-2022 transfer_checked.
    // The wrapped mint points at the dedicated `vela-transfer-hook` program, so the hook fires
    // during this CPI without re-entering the main protocol program.
    if charge_amount > 0 {
        let subscriber_key = mandate.subscriber;
        let mandate_bump = [mandate.bump];
        if legacy_layout {
            let plan_key = mandate.plan;
            let legacy_seeds: &[&[u8]] = &[
                VelaMandate::SEED_PREFIX,
                subscriber_key.as_ref(),
                plan_key.as_ref(),
                &mandate_bump,
            ];
            invoke_billing_transfer(&ctx, charge_amount, &[legacy_seeds])?;
        } else {
            let merchant_key = mandate.merchant;
            let mandate_index_bytes = mandate.mandate_index.to_le_bytes();
            let current_seeds: &[&[u8]] = &[
                VelaMandate::SEED_PREFIX,
                subscriber_key.as_ref(),
                merchant_key.as_ref(),
                mandate_index_bytes.as_ref(),
                &mandate_bump,
            ];
            invoke_billing_transfer(&ctx, charge_amount, &[current_seeds])?;
        }
    }

    let mandate_frequency = i64::try_from(mandate.frequency).map_err(|_| VelaError::Overflow)?;
    let pulls_executed = mandate
        .pulls_executed
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    mandate.pulls_executed = pulls_executed;
    mandate.next_payment_due = mandate
        .next_payment_due
        .checked_add(mandate_frequency)
        .ok_or(VelaError::Overflow)?;
    mandate.last_pull_at = clock.unix_timestamp;
    if mandate.billing_type == BillingType::Usage {
        mandate.last_billing_recorded_pull = pulls_executed;
    }
    if pulls_executed >= mandate.max_pulls {
        mandate.status = MandateStatus::Expired;
    }
    write_mandate(
        &ctx.accounts.mandate.to_account_info(),
        &mandate,
        legacy_layout,
    )?;

    if let Some((old_plan, new_plan, applied_at)) = auto_applied_change {
        emit!(MandateUpgradeFinalized {
            schema_version: 1,
            mandate: ctx.accounts.mandate.key(),
            mint: ctx.accounts.wrapped_usdc_mint.key(),
            token_symbol: event_token_symbol(
                ctx.accounts.wrapped_usdc_mint.key(),
                protocol_config.wrapped_usdc_mint(),
            ),
            old_plan,
            new_plan,
            proration_amount: 0,
            change_type: 2,
            applied_at,
            timestamp: clock.unix_timestamp,
        });
    }

    // Close PullApproval PDA and refund lamports to payer.
    let approval_info = ctx.accounts.pull_approval.to_account_info();
    let payer_info = ctx.accounts.payer.to_account_info();
    close_program_account(&approval_info, &payer_info)?;

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

fn convert_instruction(
    ix: SplInstruction,
) -> anchor_lang::solana_program::instruction::Instruction {
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

fn invoke_billing_transfer<'a, 'b, 'c, 'info>(
    ctx: &Context<'a, 'b, 'c, 'info, ExecutePull<'info>>,
    charge_amount: u64,
    signer_seed_groups: &[&[&[u8]]],
) -> Result<()> {
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
        ctx.accounts.token_config.decimals,
    )
    .map_err(map_spl_error)?;
    transfer_ix.accounts.extend_from_slice(&[
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.protocol_program.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.wrapping_vault.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.protocol_config.key()), false),
        SplAccountMeta::new(spl_pubkey(&ctx.accounts.pull_approval.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.token_config.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.system_program.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.system_program.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.system_program.key()), false),
        SplAccountMeta::new_readonly(
            spl_pubkey(&ctx.accounts.extra_account_meta_list.key()),
            false,
        ),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.hook_program.key()), false),
    ]);
    let transfer_ix = convert_instruction(transfer_ix);
    let transfer_account_infos = [
        source_info.clone(),
        mint_info.clone(),
        destination_info.clone(),
        authority_info,
        ctx.accounts.protocol_program.to_account_info(),
        ctx.accounts.wrapping_vault.to_account_info(),
        ctx.accounts.protocol_config.to_account_info(),
        ctx.accounts.pull_approval.to_account_info(),
        ctx.accounts.token_config.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.extra_account_meta_list.to_account_info(),
        ctx.accounts.hook_program.to_account_info(),
    ];
    invoke_signed(&transfer_ix, &transfer_account_infos, signer_seed_groups).map_err(Into::into)
}

fn map_spl_error(_error: SplProgramError) -> anchor_lang::error::Error {
    anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData)
}
