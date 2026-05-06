use anchor_lang::prelude::*;
use anchor_spl::token_2022::Token2022;

use crate::{
    constants::{event_token_symbol, EXTRA_ACCOUNT_METAS_SEED},
    errors::VelaError,
    instructions::{
        protocol_config_account::load_protocol_config,
        stream_account::{
            load_stream_mandate, validate_stream_mandate_address, write_stream_mandate,
        },
        stream_transfer::{
            invoke_stream_transfer, validate_stream_transfer_accounts, StreamTransferAccounts,
        },
    },
    state::{ProtocolConfig, StreamCancelled, StreamMandate, StreamStatus},
};

#[derive(Accounts)]
pub struct CancelStream<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

    /// CHECK: Source wrapped token account validated by the downstream transfer + hook path.
    #[account(mut)]
    pub subscriber_wrapped_account: UncheckedAccount<'info>,

    /// CHECK: Destination wrapped token account validated by the downstream transfer + hook path.
    #[account(mut)]
    pub merchant_wrapped_account: UncheckedAccount<'info>,

    /// CHECK: Wrapped mint validated against protocol config and mandate.
    #[account(mut)]
    pub wrapped_usdc_mint: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Slot 3 must preserve the existing TLV PullApproval derivation for compatibility.
    pub pull_approval: UncheckedAccount<'info>,

    /// CHECK: TokenConfig PDA is validated by the transfer hook.
    pub token_config: UncheckedAccount<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    /// CHECK: Wrapping vault validated against protocol config.
    #[account(mut)]
    pub wrapping_vault: UncheckedAccount<'info>,

    /// CHECK: Transfer hook program validated against protocol config.
    pub hook_program: UncheckedAccount<'info>,

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

pub fn handler(ctx: Context<CancelStream>) -> Result<()> {
    validate_stream_transfer_accounts(
        &ctx.accounts.protocol_config.to_account_info(),
        &ctx.accounts.hook_program.to_account_info(),
        &ctx.accounts.wrapped_usdc_mint.to_account_info(),
        &ctx.accounts.wrapping_vault.to_account_info(),
    )?;
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;

    let mut mandate = load_stream_mandate(&ctx.accounts.mandate.to_account_info())?;
    validate_stream_mandate_address(&ctx.accounts.mandate.key(), &mandate)?;
    require_keys_eq!(
        ctx.accounts.wrapped_usdc_mint.key(),
        mandate.mint,
        VelaError::UsdcMintMismatch
    );

    let authority = ctx.accounts.authority.key();
    require!(
        authority == mandate.subscriber || authority == mandate.merchant,
        VelaError::UnauthorizedStreamSigner
    );
    require!(
        mandate.status != StreamStatus::Cancelled,
        VelaError::StreamAlreadyCancelled
    );

    let clock_now = Clock::get()?.unix_timestamp;
    let elapsed_since_settle = clock_now
        .checked_sub(mandate.last_settled_ts)
        .ok_or(VelaError::ClockRegression)?;
    require!(elapsed_since_settle >= 0, VelaError::ClockRegression);
    let settle_amount = if mandate.status == StreamStatus::Active
        && elapsed_since_settle >= i64::from(mandate.min_settle_interval)
    {
        crate::instructions::settle_accrued_in_place(&mut mandate, clock_now)?
    } else {
        0
    };
    if settle_amount > 0 {
        let mandate_index_bytes = mandate.mandate_index.to_le_bytes();
        let mandate_bump = [mandate.bump];
        let signer_seeds: &[&[u8]] = &[
            StreamMandate::SEED_PREFIX,
            mandate.subscriber.as_ref(),
            mandate.merchant.as_ref(),
            mandate_index_bytes.as_ref(),
            &mandate_bump,
        ];
        let source_info = ctx.accounts.subscriber_wrapped_account.to_account_info();
        let mint_info = ctx.accounts.wrapped_usdc_mint.to_account_info();
        let destination_info = ctx.accounts.merchant_wrapped_account.to_account_info();
        let authority_info = ctx.accounts.mandate.to_account_info();
        let protocol_program_info = ctx.accounts.protocol_program.to_account_info();
        let wrapping_vault_info = ctx.accounts.wrapping_vault.to_account_info();
        let protocol_config_info = ctx.accounts.protocol_config.to_account_info();
        let pull_approval_info = ctx.accounts.pull_approval.to_account_info();
        let token_config_info = ctx.accounts.token_config.to_account_info();
        let system_program_info = ctx.accounts.system_program.to_account_info();
        let extra_account_meta_list_info = ctx.accounts.extra_account_meta_list.to_account_info();
        let hook_program_info = ctx.accounts.hook_program.to_account_info();
        let token_2022_program_info = ctx.accounts.token_2022_program.to_account_info();
        invoke_stream_transfer(
            StreamTransferAccounts {
                source: &source_info,
                mint: &mint_info,
                destination: &destination_info,
                authority: &authority_info,
                protocol_program: &protocol_program_info,
                wrapping_vault: &wrapping_vault_info,
                protocol_config: &protocol_config_info,
                pull_approval: &pull_approval_info,
                token_config: &token_config_info,
                system_program: &system_program_info,
                extra_account_meta_list: &extra_account_meta_list_info,
                hook_program: &hook_program_info,
                token_2022_program: &token_2022_program_info,
            },
            settle_amount,
            &[signer_seeds],
        )?;
    }

    mandate.status = StreamStatus::Cancelled;
    mandate.paused_at = None;
    write_stream_mandate(&ctx.accounts.mandate.to_account_info(), &mandate)?;

    emit!(StreamCancelled {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        mint: mandate.mint,
        token_symbol: event_token_symbol(mandate.mint, protocol_config.wrapped_usdc_mint()),
        cancelled_at: clock_now,
        signer: authority,
        final_settle_amount: settle_amount,
        total_streamed_final: mandate.total_streamed,
        timestamp: clock_now,
    });

    Ok(())
}
