use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::{
        keeper_config_account::load_keeper_config,
        mandate_account::{load_mandate_account, validate_loaded_mandate_address},
    },
    state::{KeeperConfig, MandateStatus, PullApproval},
};

#[derive(Accounts)]
pub struct DemoApprovePull<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        seeds = [KeeperConfig::SEED_PREFIX],
        bump,
    )]
    pub keeper_config: UncheckedAccount<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        space = PullApproval::SIZE,
        seeds = [PullApproval::SEED_PREFIX, mandate.key().as_ref()],
        bump,
    )]
    pub pull_approval: Account<'info, PullApproval>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<DemoApprovePull>,
    approved_amount: u64,
    ttl_seconds: i64,
) -> Result<()> {
    let loaded_mandate = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &loaded_mandate)?;
    let mandate = loaded_mandate.into_current();
    let keeper_config = load_keeper_config(&ctx.accounts.keeper_config.to_account_info())?;
    require!(
        ctx.accounts.payer.key() == keeper_config.keeper_authority()
            || ctx.accounts.payer.key() == mandate.subscriber,
        VelaError::UnauthorizedKeeper
    );
    require!(
        mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );
    require!(
        approved_amount <= mandate.amount,
        VelaError::AmountExceedsPlanAmount
    );
    require!(ttl_seconds > 0, VelaError::FrequencyTooLow);

    let now = Clock::get()?.unix_timestamp;
    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = ctx.accounts.mandate.key();
    approval.valid_until = now.checked_add(ttl_seconds).ok_or(VelaError::Overflow)?;
    approval.approved = true;
    approval.approved_amount = approved_amount;
    approval.created_at = now;
    approval.bump = ctx.bumps.pull_approval;

    emit!(DemoPullApprovedEvent {
        mandate: ctx.accounts.mandate.key(),
        approved_amount,
        valid_until: approval.valid_until,
    });

    Ok(())
}

#[event]
pub struct DemoPullApprovedEvent {
    pub mandate: Pubkey,
    pub approved_amount: u64,
    pub valid_until: i64,
}
