use anchor_lang::prelude::*;
use anchor_spl::token_2022::Token2022;

use crate::{
    constants::{event_token_symbol, EXTRA_ACCOUNT_METAS_SEED},
    errors::VelaError,
    instructions::{
        keeper_config_account::load_keeper_config,
        protocol_config_account::load_protocol_config,
        stream_account::{
            load_stream_mandate, validate_stream_mandate_address, write_stream_mandate,
        },
        stream_transfer::{
            invoke_stream_transfer, validate_stream_transfer_accounts, StreamTransferAccounts,
        },
    },
    state::{KeeperConfig, ProtocolConfig, PullApproval, StreamMandate, StreamSettled},
};

#[derive(Accounts)]
pub struct ExecuteStream<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Used for stream mandate validation only.
    pub subscriber: UncheckedAccount<'info>,

    /// CHECK: Used for stream mandate validation and merchant-signer authorization.
    pub merchant: UncheckedAccount<'info>,

    #[account(
        seeds = [KeeperConfig::SEED_PREFIX],
        bump,
    )]
    pub keeper_config: UncheckedAccount<'info>,

    #[account(mut)]
    pub stream_mandate: UncheckedAccount<'info>,

    /// CHECK: Source wrapped token account validated by the downstream transfer + hook path.
    #[account(mut)]
    pub subscriber_wrapped_account: UncheckedAccount<'info>,

    /// CHECK: Destination wrapped token account validated by the downstream transfer + hook path.
    #[account(mut)]
    pub merchant_wrapped_account: UncheckedAccount<'info>,

    /// CHECK: Wrapped mint validated against protocol config.
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

pub fn handler<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ExecuteStream<'info>>,
) -> Result<()> {
    let keeper_config = load_keeper_config(&ctx.accounts.keeper_config.to_account_info())?;
    validate_stream_transfer_accounts(
        &ctx.accounts.protocol_config.to_account_info(),
        &ctx.accounts.hook_program.to_account_info(),
        &ctx.accounts.wrapped_usdc_mint.to_account_info(),
        &ctx.accounts.wrapping_vault.to_account_info(),
    )?;

    let mut stream = load_stream_mandate(&ctx.accounts.stream_mandate.to_account_info())?;
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    validate_stream_mandate_address(&ctx.accounts.stream_mandate.key(), &stream)?;
    require_keys_eq!(ctx.accounts.subscriber.key(), stream.subscriber);
    require_keys_eq!(ctx.accounts.merchant.key(), stream.merchant);
    require_keys_eq!(
        ctx.accounts.wrapped_usdc_mint.key(),
        stream.mint,
        VelaError::UsdcMintMismatch
    );

    let payer_key = ctx.accounts.payer.key();
    let is_keeper = payer_key == keeper_config.keeper_authority();
    let is_merchant = payer_key == stream.merchant;
    let is_subscriber = payer_key == stream.subscriber;
    require!(
        is_keeper || is_merchant || is_subscriber,
        VelaError::UnauthorizedStreamSigner
    );

    let (expected_approval, _) = Pubkey::find_program_address(
        &[
            PullApproval::SEED_PREFIX,
            ctx.accounts.stream_mandate.key().as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(
        ctx.accounts.pull_approval.key(),
        expected_approval,
        VelaError::TransferNotAuthorized
    );

    let clock = Clock::get()?;
    let settle_amount =
        crate::instructions::settle_accrued_in_place(&mut stream, clock.unix_timestamp)?;
    if settle_amount == 0 {
        write_stream_mandate(&ctx.accounts.stream_mandate.to_account_info(), &stream)?;
        return Ok(());
    }

    let subscriber_key = stream.subscriber;
    let merchant_key = stream.merchant;
    let mandate_index_bytes = stream.mandate_index.to_le_bytes();
    let (_, mandate_bump) = Pubkey::find_program_address(
        &[
            StreamMandate::SEED_PREFIX,
            subscriber_key.as_ref(),
            merchant_key.as_ref(),
            mandate_index_bytes.as_ref(),
        ],
        &crate::ID,
    );
    let mandate_bump = [mandate_bump];
    let signer_seeds: &[&[u8]] = &[
        StreamMandate::SEED_PREFIX,
        subscriber_key.as_ref(),
        merchant_key.as_ref(),
        mandate_index_bytes.as_ref(),
        &mandate_bump,
    ];
    let source_info = ctx.accounts.subscriber_wrapped_account.to_account_info();
    let mint_info = ctx.accounts.wrapped_usdc_mint.to_account_info();
    let destination_info = ctx.accounts.merchant_wrapped_account.to_account_info();
    let authority_info = ctx.accounts.stream_mandate.to_account_info();
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
            expected_source_authority: ctx.accounts.stream_mandate.key(),
            expected_destination_owner: stream.merchant,
        },
        settle_amount,
        &[signer_seeds],
    )?;
    write_stream_mandate(&ctx.accounts.stream_mandate.to_account_info(), &stream)?;

    emit!(StreamSettled {
        schema_version: 1,
        mandate: ctx.accounts.stream_mandate.key(),
        mint: stream.mint,
        token_symbol: event_token_symbol(stream.mint, protocol_config.wrapped_usdc_mint()),
        amount: settle_amount,
        total_streamed_after: stream.total_streamed,
        last_settled_ts: stream.last_settled_ts,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
