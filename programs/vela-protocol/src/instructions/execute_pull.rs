use anchor_lang::prelude::*;
use anchor_spl::token::{transfer_checked, Mint, Token, TokenAccount, TransferChecked};

use crate::{
    constants::USDC_DECIMALS,
    errors::VelaError,
    state::{MandateStatus, PlanStatus, PullApproval, VelaMandate, VelaPlan},
};

#[derive(Accounts)]
pub struct ExecutePull<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Used for mandate PDA derivation and token ownership checks.
    pub subscriber: UncheckedAccount<'info>,

    /// CHECK: Used for plan PDA validation and token ownership checks.
    pub merchant: UncheckedAccount<'info>,

    #[account(
        seeds = [
            VelaPlan::SEED_PREFIX,
            merchant.key().as_ref(),
            plan.plan_id.to_le_bytes().as_ref()
        ],
        bump = plan.bump
    )]
    pub plan: Account<'info, VelaPlan>,

    #[account(
        mut,
        seeds = [
            VelaMandate::SEED_PREFIX,
            subscriber.key().as_ref(),
            plan.key().as_ref()
        ],
        bump = mandate.bump
    )]
    pub mandate: Account<'info, VelaMandate>,

    #[account(
        mut,
        constraint = subscriber_token_account.owner == subscriber.key(),
        constraint = subscriber_token_account.mint == usdc_mint.key()
    )]
    pub subscriber_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = merchant_token_account.owner == merchant.key(),
        constraint = merchant_token_account.mint == usdc_mint.key()
    )]
    pub merchant_token_account: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,

    /// CHECK: The handler validates the PDA derivation, ownership, and deserializes PullApproval manually.
    #[account(mut)]
    pub pull_approval: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<ExecutePull>) -> Result<()> {
    require!(
        ctx.accounts.plan.status == PlanStatus::Active,
        VelaError::PlanNotActive
    );
    require!(
        ctx.accounts.mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );
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
    require!(
        ctx.accounts.mandate.amount == ctx.accounts.plan.amount,
        VelaError::AmountExceedsPlanAmount
    );
    require!(
        ctx.accounts.subscriber_token_account.amount >= ctx.accounts.plan.amount,
        VelaError::InsufficientBalance
    );
    require!(
        ctx.accounts.usdc_mint.decimals == USDC_DECIMALS,
        VelaError::AmountExceedsPlanAmount
    );

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

    let approval_data = ctx.accounts.pull_approval.try_borrow_data()?;
    let mut approval_slice: &[u8] = &approval_data;
    let approval = PullApproval::try_deserialize(&mut approval_slice)
        .map_err(|_| VelaError::ApprovalNotGranted)?;

    require!(approval.approved, VelaError::ApprovalNotGranted);
    require!(
        clock.unix_timestamp <= approval.valid_until,
        VelaError::ApprovalExpired
    );

    let subscriber_key = ctx.accounts.subscriber.key();
    let plan_key = ctx.accounts.plan.key();
    let mandate_bump = [ctx.accounts.mandate.bump];
    let signer_seeds: &[&[u8]] = &[
        VelaMandate::SEED_PREFIX,
        subscriber_key.as_ref(),
        plan_key.as_ref(),
        &mandate_bump,
    ];

    let signer_seed_groups = [signer_seeds];
    let transfer_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        TransferChecked {
            from: ctx.accounts.subscriber_token_account.to_account_info(),
            mint: ctx.accounts.usdc_mint.to_account_info(),
            to: ctx.accounts.merchant_token_account.to_account_info(),
            authority: ctx.accounts.mandate.to_account_info(),
        },
        &signer_seed_groups,
    );
    transfer_checked(transfer_ctx, ctx.accounts.plan.amount, USDC_DECIMALS)?;

    let mandate = &mut ctx.accounts.mandate;
    mandate.pulls_executed = mandate
        .pulls_executed
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;
    let frequency = i64::try_from(ctx.accounts.plan.frequency).map_err(|_| VelaError::Overflow)?;
    mandate.next_payment_due = mandate
        .next_payment_due
        .checked_add(frequency)
        .ok_or(VelaError::Overflow)?;
    mandate.last_pull_at = clock.unix_timestamp;
    if mandate.pulls_executed >= mandate.max_pulls {
        mandate.status = MandateStatus::Expired;
    }

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
