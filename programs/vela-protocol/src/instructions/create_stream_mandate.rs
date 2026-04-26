use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke_signed, program_error::ProgramError, system_instruction},
};
use anchor_spl::token_interface::Mint;

use crate::{
    errors::VelaError,
    instructions::{
        merchant_account::validate_merchant_state_address, stream_account::write_stream_mandate,
    },
    state::{
        BillingRail, MerchantState, StreamCreated, StreamMandate, StreamStatus, TokenConfig,
        CURRENT_ACCOUNT_VERSION,
    },
};

#[derive(Accounts)]
pub struct CreateStreamMandate<'info> {
    #[account(mut)]
    pub subscriber: Signer<'info>,

    #[account(mut)]
    pub merchant_state: Account<'info, MerchantState>,

    #[account(mut)]
    /// CHECK: PDA is derived and initialized in the handler because the seed index is dynamic.
    pub mandate: UncheckedAccount<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        seeds = [TokenConfig::SEED_PREFIX, mint.key().as_ref()],
        bump = token_config.bump,
    )]
    pub token_config: Account<'info, TokenConfig>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<CreateStreamMandate>,
    rate_per_second: u64,
    authorized_max_rate: u64,
    max_streamed: Option<u64>,
    min_settle_interval: u32,
) -> Result<()> {
    require!(
        min_settle_interval >= 60,
        VelaError::MinSettleIntervalTooLow
    );
    require!(rate_per_second > 0, VelaError::RateMustBeNonZero);
    require!(
        rate_per_second <= authorized_max_rate,
        VelaError::AuthorizedMaxRateTooLow
    );
    require!(ctx.accounts.token_config.enabled, VelaError::TokenDisabled);
    require!(
        ctx.accounts.token_config.billing_rail == BillingRail::TransferHook,
        VelaError::InvalidBillingRail
    );
    require_keys_eq!(
        ctx.accounts.token_config.mint,
        ctx.accounts.mint.key(),
        VelaError::TokenNotRegistered
    );

    let merchant_key = ctx.accounts.merchant_state.merchant;
    validate_merchant_state_address(&ctx.accounts.merchant_state.key(), &merchant_key)?;

    let mandate_index = ctx.accounts.merchant_state.stream_mandate_counter;
    let mandate_index_bytes = mandate_index.to_le_bytes();
    let (expected_mandate, mandate_bump) = Pubkey::find_program_address(
        &[
            StreamMandate::SEED_PREFIX,
            ctx.accounts.subscriber.key().as_ref(),
            merchant_key.as_ref(),
            mandate_index_bytes.as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(ctx.accounts.mandate.key(), expected_mandate);

    if !ctx.accounts.mandate.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized.into());
    }

    let mandate_rent = Rent::get()?.minimum_balance(StreamMandate::SIZE);
    let subscriber_key = ctx.accounts.subscriber.key();
    let mandate_seeds: &[&[u8]] = &[
        StreamMandate::SEED_PREFIX,
        subscriber_key.as_ref(),
        merchant_key.as_ref(),
        mandate_index_bytes.as_ref(),
        &[mandate_bump],
    ];
    invoke_signed(
        &system_instruction::create_account(
            &ctx.accounts.payer.key(),
            &ctx.accounts.mandate.key(),
            mandate_rent,
            StreamMandate::SIZE as u64,
            &crate::ID,
        ),
        &[
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.mandate.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[mandate_seeds],
    )?;

    let clock = Clock::get()?;
    let mandate = StreamMandate {
        version: CURRENT_ACCOUNT_VERSION,
        subscriber: subscriber_key,
        merchant: merchant_key,
        mint: ctx.accounts.mint.key(),
        rate_per_second,
        authorized_max_rate,
        last_settled_ts: clock.unix_timestamp,
        total_streamed: 0,
        max_streamed,
        paused_at: None,
        min_settle_interval,
        status: StreamStatus::Active,
        mandate_index,
        bump: mandate_bump,
        pending_new_rate_per_second: 0,
        pending_new_authorized_max_rate: 0,
        pending_effective_at: 0,
        pending_change_type: 0,
        pending_nonce_short: [0; 8],
        _reserved_v2: [0; 23],
    };
    write_stream_mandate(&ctx.accounts.mandate.to_account_info(), &mandate)?;

    ctx.accounts.merchant_state.stream_mandate_counter = ctx
        .accounts
        .merchant_state
        .stream_mandate_counter
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;

    emit!(StreamCreated {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        subscriber: mandate.subscriber,
        merchant: mandate.merchant,
        mint: mandate.mint,
        token_symbol: String::new(),
        rate_per_second: mandate.rate_per_second,
        authorized_max_rate: mandate.authorized_max_rate,
        max_streamed: mandate.max_streamed,
        min_settle_interval: mandate.min_settle_interval,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
