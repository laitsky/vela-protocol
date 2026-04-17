use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::mandate_account::{load_mandate_account, validate_loaded_mandate_address},
    state::MandateStatus,
};

#[derive(Accounts)]
pub struct CloseMandate<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Must be the mandate subscriber and receives rent refund.
    #[account(mut)]
    pub subscriber: UncheckedAccount<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<CloseMandate>) -> Result<()> {
    let mandate = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &mandate)?;

    let subscriber_key = mandate.subscriber();
    require_keys_eq!(
        ctx.accounts.authority.key(),
        subscriber_key,
        VelaError::UnauthorizedCancel
    );
    require_keys_eq!(
        ctx.accounts.subscriber.key(),
        subscriber_key,
        VelaError::UnauthorizedCancel
    );
    require!(
        matches!(
            mandate.status(),
            MandateStatus::Cancelled | MandateStatus::Expired
        ),
        VelaError::MigrationPreconditionFailed
    );

    let subscriber_info = ctx.accounts.subscriber.to_account_info();
    let mandate_info = ctx.accounts.mandate.to_account_info();
    let refund = mandate_info.lamports();
    **subscriber_info.lamports.borrow_mut() = subscriber_info
        .lamports()
        .checked_add(refund)
        .ok_or(VelaError::Overflow)?;
    **mandate_info.lamports.borrow_mut() = 0;

    Ok(())
}
