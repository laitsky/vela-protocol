use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke_signed, system_instruction},
};

use crate::{
    errors::VelaError,
    instructions::{
        account_close::close_program_account,
        mandate_account::{load_mandate_account, validate_loaded_mandate_address, write_mandate},
        plan_account::load_plan_account,
        protocol_config_account::load_protocol_config,
    },
    state::{MerchantState, ProtocolConfig, VelaMandate, CURRENT_ACCOUNT_VERSION},
};

#[derive(Accounts)]
pub struct MigrateMandate<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [MerchantState::SEED_PREFIX, merchant.key().as_ref()],
        bump,
    )]
    pub merchant_state: Box<Account<'info, MerchantState>>,

    /// CHECK: Validated against legacy mandate bytes.
    pub subscriber: UncheckedAccount<'info>,

    /// CHECK: Validated against legacy mandate bytes.
    pub merchant: UncheckedAccount<'info>,

    /// CHECK: Deserialized and validated manually to support flat/usage plans.
    pub plan: UncheckedAccount<'info>,

    #[account(mut)]
    pub legacy_mandate: UncheckedAccount<'info>,

    #[account(mut)]
    pub migrated_mandate: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<MigrateMandate>) -> Result<()> {
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require_keys_eq!(
        ctx.accounts.admin.key(),
        protocol_config.admin(),
        VelaError::UnauthorizedAdmin
    );

    let loaded = load_mandate_account(&ctx.accounts.legacy_mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.legacy_mandate.key(), &loaded)?;
    require!(loaded.is_legacy(), VelaError::MigrationPreconditionFailed);

    let mut mandate = loaded.into_current();
    require_keys_eq!(ctx.accounts.subscriber.key(), mandate.subscriber);
    require_keys_eq!(ctx.accounts.merchant.key(), mandate.merchant);
    require_keys_eq!(ctx.accounts.plan.key(), mandate.plan);
    let plan = load_plan_account(&ctx.accounts.plan.to_account_info())?;
    require_keys_eq!(plan.merchant(), mandate.merchant);

    // Reject migration when mandate-keyed dependent namespaces could be stranded.
    // PullApproval, UsageReport, and BillingEvent namespaces all key by mandate address.
    // Non-zero request/billing counters or pull history indicate dependent state has been used.
    let has_stranded_namespaces = mandate.validation_request_nonce > 0
        || mandate.billing_request_nonce > 0
        || mandate.pulls_executed > 0
        || mandate.last_billing_recorded_pull > 0;
    require!(
        !has_stranded_namespaces,
        VelaError::MigrationPreconditionFailed
    );

    let mandate_index = ctx.accounts.merchant_state.mandate_counter;
    let (expected_migrated, migrated_bump) = Pubkey::find_program_address(
        &[
            VelaMandate::SEED_PREFIX,
            mandate.subscriber.as_ref(),
            mandate.merchant.as_ref(),
            mandate_index.to_le_bytes().as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(ctx.accounts.migrated_mandate.key(), expected_migrated);
    require!(
        ctx.accounts.migrated_mandate.data_is_empty(),
        VelaError::MigrationPreconditionFailed
    );

    let lamports = ctx.accounts.rent.minimum_balance(VelaMandate::SIZE);
    let mandate_index_bytes = mandate_index.to_le_bytes();
    let signer_seeds: &[&[u8]] = &[
        VelaMandate::SEED_PREFIX,
        mandate.subscriber.as_ref(),
        mandate.merchant.as_ref(),
        mandate_index_bytes.as_ref(),
        &[migrated_bump],
    ];
    invoke_signed(
        &system_instruction::create_account(
            &ctx.accounts.admin.key(),
            &ctx.accounts.migrated_mandate.key(),
            lamports,
            VelaMandate::SIZE as u64,
            &crate::ID,
        ),
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.migrated_mandate.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[signer_seeds],
    )?;

    mandate.bump = migrated_bump;
    mandate.mandate_index = mandate_index;
    mandate.version = CURRENT_ACCOUNT_VERSION;
    mandate.credit_balance = 0;
    mandate.pending_new_plan = Pubkey::default();
    mandate.pending_effective_at = 0;
    mandate.pending_change_type = 0;
    mandate.pending_nonce_short = [0; 8];
    mandate._reserved_v3 = [0; 7];
    write_mandate(
        &ctx.accounts.migrated_mandate.to_account_info(),
        &mandate,
        false,
    )?;

    ctx.accounts.merchant_state.mandate_counter = ctx
        .accounts
        .merchant_state
        .mandate_counter
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;

    let legacy_info = ctx.accounts.legacy_mandate.to_account_info();
    let admin_info = ctx.accounts.admin.to_account_info();
    close_program_account(&legacy_info, &admin_info)?;

    Ok(())
}
