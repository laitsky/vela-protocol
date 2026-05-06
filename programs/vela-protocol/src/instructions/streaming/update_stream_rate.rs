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
    state::{
        MandateUpgradeFinalized, MandateUpgradeInitiated, ProtocolConfig, StreamMandate,
        StreamRateUpdated, StreamStatus,
    },
};

#[derive(Accounts)]
pub struct UpdateStreamRate<'info> {
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

pub fn handler(
    ctx: Context<UpdateStreamRate>,
    new_rate: Option<u64>,
    new_authorized_max_rate: Option<u64>,
) -> Result<()> {
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
    require!(
        mandate.status == StreamStatus::Active,
        VelaError::StreamNotActive
    );

    let authority = ctx.accounts.authority.key();
    let is_subscriber = authority == mandate.subscriber;
    let is_merchant = authority == mandate.merchant;
    require!(
        is_subscriber || is_merchant,
        VelaError::UnauthorizedStreamSigner
    );
    require!(
        new_rate.is_some() || new_authorized_max_rate.is_some(),
        VelaError::NoUpdateProvided
    );

    let subscriber_required = new_authorized_max_rate.is_some()
        || matches!(new_rate, Some(next_rate) if next_rate > mandate.authorized_max_rate);
    if subscriber_required {
        require!(is_subscriber, VelaError::UnauthorizedStreamSigner);
    }

    if let Some(next_rate) = new_rate {
        require!(next_rate > 0, VelaError::RateMustBeNonZero);
    }

    let clock_now = Clock::get()?.unix_timestamp;
    let settle_amount = crate::instructions::settle_accrued_in_place(&mut mandate, clock_now)?;
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

    let old_rate = mandate.rate_per_second;
    let old_authorized_max_rate = mandate.authorized_max_rate;
    if let Some(next_rate) = new_rate {
        mandate.rate_per_second = next_rate;
    }
    if let Some(next_authorized_max_rate) = new_authorized_max_rate {
        mandate.authorized_max_rate = next_authorized_max_rate;
    }
    require!(
        mandate.rate_per_second <= mandate.authorized_max_rate,
        VelaError::AuthorizedMaxRateTooLow
    );
    write_stream_mandate(&ctx.accounts.mandate.to_account_info(), &mandate)?;

    let proration_amount = i64::try_from(settle_amount).map_err(|_| VelaError::Overflow)?;

    emit!(MandateUpgradeInitiated {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        mint: mandate.mint,
        token_symbol: event_token_symbol(mandate.mint, protocol_config.wrapped_usdc_mint()),
        old_plan: Pubkey::default(),
        new_plan: Pubkey::default(),
        proration_amount,
        change_type: 1,
        signer: authority,
        applied_at: clock_now,
        timestamp: clock_now,
    });

    emit!(StreamRateUpdated {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        mint: mandate.mint,
        token_symbol: event_token_symbol(mandate.mint, protocol_config.wrapped_usdc_mint()),
        old_rate_per_second: old_rate,
        new_rate_per_second: mandate.rate_per_second,
        old_authorized_max_rate,
        new_authorized_max_rate: mandate.authorized_max_rate,
        signer: authority,
        final_settle_amount: settle_amount,
        timestamp: clock_now,
    });

    emit!(MandateUpgradeFinalized {
        schema_version: 1,
        mandate: ctx.accounts.mandate.key(),
        mint: mandate.mint,
        token_symbol: event_token_symbol(mandate.mint, protocol_config.wrapped_usdc_mint()),
        old_plan: Pubkey::default(),
        new_plan: Pubkey::default(),
        proration_amount,
        change_type: 1,
        applied_at: clock_now,
        timestamp: clock_now,
    });

    Ok(())
}
