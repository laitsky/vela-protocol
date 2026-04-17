use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::token_2022::Token2022;
use solana_instruction::{AccountMeta as SplAccountMeta, Instruction as SplInstruction};
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;

use crate::{
    constants::{EXTRA_ACCOUNT_METAS_SEED, USDC_DECIMALS, WRAPPED_USDC_SYMBOL},
    errors::VelaError,
    instructions::{
        keeper_config_account::load_keeper_config,
        protocol_config_account::load_protocol_config,
        stream_account::{
            load_stream_mandate, validate_stream_mandate_address, write_stream_mandate,
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
    validate_stream_mandate_address(&ctx.accounts.stream_mandate.key(), &stream)?;
    require_keys_eq!(ctx.accounts.subscriber.key(), stream.subscriber);
    require_keys_eq!(ctx.accounts.merchant.key(), stream.merchant);

    let payer_key = ctx.accounts.payer.key();
    let is_keeper = payer_key == keeper_config.keeper_authority();
    let is_merchant = payer_key == stream.merchant;
    require!(
        is_keeper || is_merchant,
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
    invoke_stream_transfer(
        &ctx.accounts.subscriber_wrapped_account.to_account_info(),
        &ctx.accounts.wrapped_usdc_mint.to_account_info(),
        &ctx.accounts.merchant_wrapped_account.to_account_info(),
        &ctx.accounts.stream_mandate.to_account_info(),
        &ctx.accounts.protocol_program.to_account_info(),
        &ctx.accounts.wrapping_vault.to_account_info(),
        &ctx.accounts.protocol_config.to_account_info(),
        &ctx.accounts.pull_approval.to_account_info(),
        &ctx.accounts.token_config.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.extra_account_meta_list.to_account_info(),
        &ctx.accounts.hook_program.to_account_info(),
        &ctx.accounts.token_2022_program.to_account_info(),
        settle_amount,
        &[signer_seeds],
    )?;
    write_stream_mandate(&ctx.accounts.stream_mandate.to_account_info(), &stream)?;

    emit!(StreamSettled {
        schema_version: 1,
        mandate: ctx.accounts.stream_mandate.key(),
        mint: stream.mint,
        token_symbol: WRAPPED_USDC_SYMBOL.to_string(),
        amount: settle_amount,
        total_streamed_after: stream.total_streamed,
        last_settled_ts: stream.last_settled_ts,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

fn spl_pubkey(key: &Pubkey) -> SplPubkey {
    SplPubkey::from(key.to_bytes())
}

fn anchor_pubkey(key: SplPubkey) -> Pubkey {
    Pubkey::new_from_array(key.to_bytes())
}

fn convert_instruction(
    ix: SplInstruction,
) -> anchor_lang::solana_program::instruction::Instruction {
    anchor_lang::solana_program::instruction::Instruction {
        program_id: anchor_pubkey(ix.program_id),
        accounts: ix
            .accounts
            .into_iter()
            .map(|meta| {
                if meta.is_writable {
                    anchor_lang::solana_program::instruction::AccountMeta::new(
                        anchor_pubkey(meta.pubkey),
                        meta.is_signer,
                    )
                } else {
                    anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
                        anchor_pubkey(meta.pubkey),
                        meta.is_signer,
                    )
                }
            })
            .collect(),
        data: ix.data,
    }
}

pub(crate) fn validate_stream_transfer_accounts(
    protocol_config_info: &AccountInfo<'_>,
    hook_program_info: &AccountInfo<'_>,
    wrapped_usdc_mint_info: &AccountInfo<'_>,
    wrapping_vault_info: &AccountInfo<'_>,
) -> Result<()> {
    let protocol_config = load_protocol_config(protocol_config_info)?;
    require!(!protocol_config.paused(), VelaError::ProtocolPaused);
    let hook_program_id = protocol_config.transfer_hook_program_id();
    require!(
        hook_program_id != Pubkey::default(),
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        hook_program_info.key(),
        hook_program_id,
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        wrapped_usdc_mint_info.key(),
        protocol_config.wrapped_usdc_mint(),
        VelaError::UsdcMintMismatch
    );
    require_keys_eq!(
        wrapping_vault_info.key(),
        protocol_config.wrapping_vault(),
        VelaError::VaultMismatch
    );
    Ok(())
}

pub(crate) fn invoke_stream_transfer<'info>(
    source_info: &AccountInfo<'info>,
    mint_info: &AccountInfo<'info>,
    destination_info: &AccountInfo<'info>,
    authority_info: &AccountInfo<'info>,
    protocol_program_info: &AccountInfo<'info>,
    wrapping_vault_info: &AccountInfo<'info>,
    protocol_config_info: &AccountInfo<'info>,
    pull_approval_info: &AccountInfo<'info>,
    token_config_info: &AccountInfo<'info>,
    system_program_info: &AccountInfo<'info>,
    extra_account_meta_list_info: &AccountInfo<'info>,
    hook_program_info: &AccountInfo<'info>,
    token_2022_program_info: &AccountInfo<'info>,
    amount: u64,
    signer_seed_groups: &[&[&[u8]]],
) -> Result<()> {
    let mut transfer_ix = spl_token_2022::instruction::transfer_checked(
        &spl_pubkey(token_2022_program_info.key),
        &spl_pubkey(source_info.key),
        &spl_pubkey(mint_info.key),
        &spl_pubkey(destination_info.key),
        &spl_pubkey(authority_info.key),
        &[],
        amount,
        USDC_DECIMALS,
    )
    .map_err(map_spl_error)?;
    transfer_ix.accounts.extend_from_slice(&[
        SplAccountMeta::new_readonly(spl_pubkey(protocol_program_info.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(wrapping_vault_info.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(protocol_config_info.key), false),
        SplAccountMeta::new(spl_pubkey(pull_approval_info.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(token_config_info.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(system_program_info.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(system_program_info.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(system_program_info.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(extra_account_meta_list_info.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(hook_program_info.key), false),
    ]);
    let transfer_ix = convert_instruction(transfer_ix);
    let transfer_account_infos = [
        source_info.clone(),
        mint_info.clone(),
        destination_info.clone(),
        authority_info.clone(),
        protocol_program_info.clone(),
        wrapping_vault_info.clone(),
        protocol_config_info.clone(),
        pull_approval_info.clone(),
        token_config_info.clone(),
        system_program_info.clone(),
        system_program_info.clone(),
        system_program_info.clone(),
        extra_account_meta_list_info.clone(),
        hook_program_info.clone(),
    ];
    invoke_signed(&transfer_ix, &transfer_account_infos, signer_seed_groups).map_err(Into::into)
}

fn map_spl_error(_error: SplProgramError) -> anchor_lang::error::Error {
    anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData)
}
