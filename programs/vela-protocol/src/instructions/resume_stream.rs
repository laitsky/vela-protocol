use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::stream_account::{
        load_stream_mandate, validate_stream_mandate_address, write_stream_mandate,
    },
    state::{StreamResumed, StreamStatus},
};

#[derive(Accounts)]
pub struct ResumeStream<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<ResumeStream>) -> Result<()> {
    let mut mandate = load_stream_mandate(&ctx.accounts.mandate.to_account_info())?;
    validate_stream_mandate_address(&ctx.accounts.mandate.key(), &mandate)?;

    let authority = ctx.accounts.authority.key();
    require!(
        authority == mandate.subscriber || authority == mandate.merchant,
        VelaError::UnauthorizedStreamSigner
    );

    // INVARIANT EXCEPTION (RESEARCH.md Pitfall 2 / STREAM-04):
    // resume_stream is the ONLY instruction that intentionally skips settle_accrued_in_place.
    // Calling the helper here would back-accrue across the pause window, violating
    // "no back-accrual during pause" (ROADMAP success criterion #3).
    match mandate.status {
        StreamStatus::Paused => {}
        StreamStatus::Active => return Err(VelaError::StreamNotActive.into()),
        StreamStatus::Cancelled => return Err(VelaError::StreamAlreadyCancelled.into()),
    }

    require!(mandate.paused_at.is_some(), VelaError::StreamNotActive);
    let paused_at = mandate.paused_at.expect("checked above");
    let clock_now = Clock::get()?.unix_timestamp;
    let pause_duration = clock_now
        .checked_sub(paused_at)
        .ok_or(VelaError::ClockRegression)?;
    require!(pause_duration >= 0, VelaError::ClockRegression);

    mandate.last_settled_ts = clock_now;
    mandate.paused_at = None;
    mandate.status = StreamStatus::Active;
    write_stream_mandate(&ctx.accounts.mandate.to_account_info(), &mandate)?;

    emit!(StreamResumed {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        resumed_at: clock_now,
        pause_duration_secs: pause_duration as u64,
        signer: authority,
        timestamp: clock_now,
    });

    Ok(())
}
