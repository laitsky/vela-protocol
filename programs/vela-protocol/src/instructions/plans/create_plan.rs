use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke_signed, program_error::ProgramError, system_instruction},
};
use anchor_spl::token_interface::Mint;

use crate::{
    constants::MIN_FREQUENCY_SECONDS,
    errors::VelaError,
    instructions::merchant_account::{
        ensure_merchant_state, resolve_merchant_credential_mint, write_merchant_state,
    },
    instructions::{plan_account::write_plan, spl_helpers::anchor_pubkey},
    state::{
        BillingRail, MerchantState, PlanStatus, TokenConfig, VelaPlan, CURRENT_ACCOUNT_VERSION,
        PLAN_RESERVED_BYTES,
    },
};

#[derive(Accounts)]
pub struct CreatePlan<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(mut, seeds = [MerchantState::SEED_PREFIX, merchant.key().as_ref()], bump)]
    pub merchant_state: UncheckedAccount<'info>,

    #[account(mut)]
    pub plan: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: PDA mint account is created via manual CPI and constrained by seeds.
    pub credential_mint: UncheckedAccount<'info>,

    pub billing_mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [TokenConfig::SEED_PREFIX, billing_mint.key().as_ref()],
        bump = token_config.bump,
    )]
    pub token_config: Account<'info, TokenConfig>,

    pub system_program: Program<'info, System>,

    /// CHECK: Address constraint ensures this is the Token-2022 program.
    pub token_2022_program: UncheckedAccount<'info>,

    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<CreatePlan>,
    amount: u64,
    frequency: u64,
    trial_period: u64,
    max_pulls: u64,
) -> Result<()> {
    require!(
        frequency >= MIN_FREQUENCY_SECONDS,
        VelaError::FrequencyTooLow
    );
    require!(max_pulls > 0, VelaError::MaxPullsTooLow);
    require!(ctx.accounts.token_config.enabled, VelaError::TokenDisabled);
    require!(
        ctx.accounts.token_config.billing_rail == BillingRail::TransferHook,
        VelaError::InvalidBillingRail
    );
    require_keys_eq!(
        ctx.accounts.token_config.mint,
        ctx.accounts.billing_mint.key(),
        VelaError::TokenNotRegistered
    );

    let token_2022_program_id = spl_token_2022::id();
    require_keys_eq!(
        ctx.accounts.token_2022_program.key(),
        anchor_pubkey(token_2022_program_id)
    );

    let merchant_key = ctx.accounts.merchant.key();
    let merchant_state_info = ctx.accounts.merchant_state.to_account_info();
    let mut merchant_state = ensure_merchant_state(
        &ctx.accounts.merchant.to_account_info(),
        &merchant_state_info,
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.rent,
        &merchant_key,
        ctx.bumps.merchant_state,
    )?;
    let plan_id = merchant_state.plan_count;
    let plan_id_bytes = plan_id.to_le_bytes();
    let plan_key = ctx.accounts.plan.key();
    let credential_mint_key = ctx.accounts.credential_mint.key();
    let (expected_plan_key, plan_bump) = Pubkey::find_program_address(
        &[
            VelaPlan::SEED_PREFIX,
            merchant_key.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &crate::ID,
    );
    if plan_key != expected_plan_key {
        return Err(ProgramError::InvalidSeeds.into());
    }
    let (expected_credential_key, _) = Pubkey::find_program_address(
        &[b"credential", merchant_key.as_ref(), plan_id_bytes.as_ref()],
        &crate::ID,
    );
    if credential_mint_key != expected_credential_key {
        return Err(ProgramError::InvalidSeeds.into());
    }
    if !ctx.accounts.plan.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized.into());
    }

    // Resolve credential mint: merchant-first (D-10), plan-scoped fallback (CRED-05)
    let resolved_credential_mint = resolve_merchant_credential_mint(
        &merchant_state_info,
        &merchant_key,
        &credential_mint_key,
    )?;
    require!(
        merchant_state.credential_mint != Pubkey::default()
            && resolved_credential_mint == merchant_state.credential_mint,
        VelaError::MigrationPreconditionFailed
    );

    let plan_signer_seeds: &[&[u8]] = &[
        VelaPlan::SEED_PREFIX,
        merchant_key.as_ref(),
        plan_id_bytes.as_ref(),
        &[plan_bump],
    ];
    let plan_rent = ctx.accounts.rent.minimum_balance(VelaPlan::SIZE);

    invoke_signed(
        &system_instruction::create_account(
            &merchant_key,
            &plan_key,
            plan_rent,
            VelaPlan::SIZE as u64,
            &crate::ID,
        ),
        &[
            ctx.accounts.merchant.to_account_info(),
            ctx.accounts.plan.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[plan_signer_seeds],
    )?;

    let plan_state = VelaPlan {
        merchant: merchant_key,
        plan_id,
        amount,
        frequency,
        trial_period,
        max_pulls,
        status: PlanStatus::Active,
        credential_mint: resolved_credential_mint,
        billing_mint: ctx.accounts.billing_mint.key(),
        bump: plan_bump,
        version: CURRENT_ACCOUNT_VERSION,
        _reserved: [0; PLAN_RESERVED_BYTES],
    };
    write_plan(&ctx.accounts.plan.to_account_info(), &plan_state)?;

    merchant_state.plan_count = merchant_state
        .plan_count
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    write_merchant_state(&merchant_state_info, &merchant_state)?;

    Ok(())
}
