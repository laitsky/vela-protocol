use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::token_2022::Token2022;
use solana_instruction::{AccountMeta as SplAccountMeta, Instruction as SplInstruction};
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;

use crate::{
    constants::{EXTRA_ACCOUNT_METAS_SEED, USDC_DECIMALS},
    errors::VelaError,
    instructions::{keeper_config_account::load_keeper_config, protocol_config_account::load_protocol_config},
    state::{
        KeeperConfig, MerchantState, ProtocolConfig, PullApproval, StreamMandateProto,
        StreamStatusProto, ACCOUNT_RESERVED_BYTES,
    },
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateStreamMandateProtoArgs {
    pub rate: u64,
    pub max_rate: u64,
    pub cap: Option<u64>,
    pub min_interval: u32,
}

#[derive(Accounts)]
pub struct CreateStreamMandateProto<'info> {
    #[account(mut)]
    pub subscriber: Signer<'info>,

    /// CHECK: Used for PDA validation only.
    pub merchant: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [MerchantState::SEED_PREFIX, merchant.key().as_ref()],
        bump
    )]
    pub merchant_state: Box<Account<'info, MerchantState>>,

    #[account(mut)]
    /// CHECK: Handler validates and initializes the PDA.
    pub mandate: UncheckedAccount<'info>,

    /// CHECK: Wrapped mint resolved from protocol config for the throwaway proto.
    pub mint: UncheckedAccount<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn create_stream_mandate_handler(
    ctx: Context<CreateStreamMandateProto>,
    args: CreateStreamMandateProtoArgs,
) -> Result<()> {
    require!(args.rate > 0, VelaError::InvalidAmount);
    require!(args.max_rate >= args.rate, VelaError::AmountExceedsPlanAmount);
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require_keys_eq!(
        ctx.accounts.mint.key(),
        protocol_config.wrapped_usdc_mint(),
        VelaError::UsdcMintMismatch
    );

    let mandate_index = ctx.accounts.merchant_state.mandate_counter;
    let mandate_index_bytes = mandate_index.to_le_bytes();
    let subscriber_key = ctx.accounts.subscriber.key();
    let merchant_key = ctx.accounts.merchant.key();
    let (expected_mandate, mandate_bump) = Pubkey::find_program_address(
        &[
            StreamMandateProto::SEED_PREFIX,
            subscriber_key.as_ref(),
            merchant_key.as_ref(),
            mandate_index_bytes.as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(ctx.accounts.mandate.key(), expected_mandate);
    if !ctx.accounts.mandate.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized.into());
    }

    let lamports = Rent::get()?.minimum_balance(StreamMandateProto::SIZE);
    let mandate_signer_seeds: &[&[u8]] = &[
        StreamMandateProto::SEED_PREFIX,
        subscriber_key.as_ref(),
        merchant_key.as_ref(),
        mandate_index_bytes.as_ref(),
        &[mandate_bump],
    ];
    invoke_signed(
        &anchor_lang::solana_program::system_instruction::create_account(
            &subscriber_key,
            &ctx.accounts.mandate.key(),
            lamports,
            StreamMandateProto::SIZE as u64,
            &crate::ID,
        ),
        &[
            ctx.accounts.subscriber.to_account_info(),
            ctx.accounts.mandate.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[mandate_signer_seeds],
    )?;

    let clock = Clock::get()?;
    let mandate = StreamMandateProto {
        version: StreamMandateProto::current_version(),
        subscriber: subscriber_key,
        merchant: merchant_key,
        mint: ctx.accounts.mint.key(),
        rate_per_second: args.rate,
        authorized_max_rate: args.max_rate,
        last_settled_ts: clock.unix_timestamp,
        total_streamed: 0,
        max_streamed: args.cap,
        paused_at: None,
        min_settle_interval: args.min_interval,
        status: StreamStatusProto::Active,
        mandate_index,
        bump: mandate_bump,
        _reserved: [0; ACCOUNT_RESERVED_BYTES - 8],
    };
    write_stream_mandate_proto(&ctx.accounts.mandate.to_account_info(), &mandate)?;
    ctx.accounts.merchant_state.mandate_counter = ctx
        .accounts
        .merchant_state
        .mandate_counter
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;

    Ok(())
}

#[derive(Accounts)]
pub struct ExecuteStreamProto<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Used for PDA validation only.
    pub subscriber: UncheckedAccount<'info>,

    /// CHECK: Used for PDA validation only.
    pub merchant: UncheckedAccount<'info>,

    #[account(
        seeds = [KeeperConfig::SEED_PREFIX],
        bump,
    )]
    pub keeper_config: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Deserialized manually for the prototype.
    pub stream_mandate: UncheckedAccount<'info>,

    #[account(mut)]
    pub subscriber_wrapped_account: UncheckedAccount<'info>,

    #[account(mut)]
    pub merchant_wrapped_account: UncheckedAccount<'info>,

    pub wrapped_usdc_mint: UncheckedAccount<'info>,

    pub pull_approval: UncheckedAccount<'info>,

    pub token_config: UncheckedAccount<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    pub wrapping_vault: UncheckedAccount<'info>,

    pub hook_program: UncheckedAccount<'info>,

    #[account(
        seeds = [EXTRA_ACCOUNT_METAS_SEED, wrapped_usdc_mint.key().as_ref()],
        bump,
        seeds::program = hook_program.key(),
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    #[account(address = crate::ID)]
    pub protocol_program: UncheckedAccount<'info>,

    pub token_2022_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn execute_stream_handler<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ExecuteStreamProto<'info>>,
) -> Result<()> {
    let keeper_config = load_keeper_config(&ctx.accounts.keeper_config.to_account_info())?;
    require_keys_eq!(
        ctx.accounts.payer.key(),
        keeper_config.keeper_authority(),
        VelaError::UnauthorizedKeeper
    );

    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require!(!protocol_config.paused(), VelaError::ProtocolPaused);
    require_keys_eq!(
        ctx.accounts.hook_program.key(),
        protocol_config.transfer_hook_program_id(),
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        ctx.accounts.wrapped_usdc_mint.key(),
        protocol_config.wrapped_usdc_mint(),
        VelaError::UsdcMintMismatch
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.key(),
        protocol_config.wrapping_vault(),
        VelaError::VaultMismatch
    );

    let mut mandate = load_stream_mandate_proto(&ctx.accounts.stream_mandate.to_account_info())?;
    validate_stream_mandate_proto_address(&ctx.accounts.stream_mandate.key(), &mandate)?;
    require!(mandate.status == StreamStatusProto::Active, VelaError::MandateNotActive);
    require_keys_eq!(ctx.accounts.subscriber.key(), mandate.subscriber);
    require_keys_eq!(ctx.accounts.merchant.key(), mandate.merchant);
    require_keys_eq!(ctx.accounts.wrapped_usdc_mint.key(), mandate.mint);
    let expected_pull_approval = Pubkey::find_program_address(
        &[PullApproval::SEED_PREFIX, ctx.accounts.stream_mandate.key().as_ref()],
        &crate::ID,
    )
    .0;
    require_keys_eq!(
        ctx.accounts.pull_approval.key(),
        expected_pull_approval,
        VelaError::ApprovalNotGranted
    );

    let clock = Clock::get()?;
    let amount = settle_accrued_in_place(&mut mandate, clock.unix_timestamp)?;
    if amount > 0 {
        let subscriber_key = mandate.subscriber;
        let merchant_key = mandate.merchant;
        let mandate_index_bytes = mandate.mandate_index.to_le_bytes();
        let mandate_bump = [mandate.bump];
        let signer_seeds: &[&[u8]] = &[
            StreamMandateProto::SEED_PREFIX,
            subscriber_key.as_ref(),
            merchant_key.as_ref(),
            mandate_index_bytes.as_ref(),
            &mandate_bump,
        ];
        invoke_stream_transfer(&ctx, amount, &[signer_seeds])?;
    }
    write_stream_mandate_proto(&ctx.accounts.stream_mandate.to_account_info(), &mandate)?;

    Ok(())
}

pub(crate) fn settle_accrued_in_place(
    mandate: &mut StreamMandateProto,
    clock_now: i64,
) -> Result<u64> {
    if mandate.status != StreamStatusProto::Active || mandate.paused_at.is_some() {
        return Ok(0);
    }

    let elapsed_i64 = clock_now
        .checked_sub(mandate.last_settled_ts)
        .ok_or(VelaError::Overflow)?;
    require!(elapsed_i64 >= 0, VelaError::Overflow);
    let elapsed = u128::from(u64::try_from(elapsed_i64).map_err(|_| VelaError::Overflow)?);
    let gross = elapsed
        .checked_mul(u128::from(mandate.rate_per_second))
        .ok_or(VelaError::Overflow)?;
    let remaining = match mandate.max_streamed {
        Some(cap) => u128::from(cap)
            .checked_sub(u128::from(mandate.total_streamed))
            .ok_or(VelaError::AmountExceedsPlanAmount)?,
        None => u128::MAX,
    };
    let amount = u64::try_from(core::cmp::min(gross, remaining)).map_err(|_| VelaError::Overflow)?;
    mandate.total_streamed = mandate
        .total_streamed
        .checked_add(amount)
        .ok_or(VelaError::Overflow)?;
    mandate.last_settled_ts = clock_now;
    Ok(amount)
}

pub fn load_stream_mandate_proto(stream_info: &AccountInfo<'_>) -> Result<StreamMandateProto> {
    require_keys_eq!(*stream_info.owner, crate::ID, VelaError::InvalidProtocolConfig);
    if stream_info.data_is_empty() {
        return Err(ProgramError::UninitializedAccount.into());
    }
    let data = stream_info.try_borrow_data()?;
    let mut slice: &[u8] = &data;
    StreamMandateProto::try_deserialize(&mut slice)
        .map_err(|_| ProgramError::InvalidAccountData.into())
}

pub fn validate_stream_mandate_proto_address(
    mandate_key: &Pubkey,
    mandate: &StreamMandateProto,
) -> Result<()> {
    let expected = Pubkey::find_program_address(
        &[
            StreamMandateProto::SEED_PREFIX,
            mandate.subscriber.as_ref(),
            mandate.merchant.as_ref(),
            mandate.mandate_index.to_le_bytes().as_ref(),
        ],
        &crate::ID,
    )
    .0;
    require_keys_eq!(*mandate_key, expected);
    Ok(())
}

pub fn write_stream_mandate_proto(
    mandate_info: &AccountInfo<'_>,
    mandate: &StreamMandateProto,
) -> Result<()> {
    let mut data = mandate_info.try_borrow_mut_data()?;
    data.fill(0);
    let mut slice: &mut [u8] = &mut data[..];
    mandate.try_serialize(&mut slice)?;
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

fn invoke_stream_transfer<'a, 'b, 'c, 'info>(
    ctx: &Context<'a, 'b, 'c, 'info, ExecuteStreamProto<'info>>,
    amount: u64,
    signer_seed_groups: &[&[&[u8]]],
) -> Result<()> {
    let source_info = ctx.accounts.subscriber_wrapped_account.to_account_info();
    let mint_info = ctx.accounts.wrapped_usdc_mint.to_account_info();
    let destination_info = ctx.accounts.merchant_wrapped_account.to_account_info();
    let authority_info = ctx.accounts.stream_mandate.to_account_info();
    let mut transfer_ix = spl_token_2022::instruction::transfer_checked(
        &spl_pubkey(ctx.accounts.token_2022_program.key),
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
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.protocol_program.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.wrapping_vault.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.protocol_config.key()), false),
        SplAccountMeta::new(spl_pubkey(&ctx.accounts.pull_approval.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.token_config.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.system_program.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.system_program.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.system_program.key()), false),
        SplAccountMeta::new_readonly(
            spl_pubkey(&ctx.accounts.extra_account_meta_list.key()),
            false,
        ),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.hook_program.key()), false),
    ]);
    let transfer_ix = convert_instruction(transfer_ix);
    let transfer_account_infos = [
        source_info.clone(),
        mint_info.clone(),
        destination_info.clone(),
        authority_info,
        ctx.accounts.protocol_program.to_account_info(),
        ctx.accounts.wrapping_vault.to_account_info(),
        ctx.accounts.protocol_config.to_account_info(),
        ctx.accounts.pull_approval.to_account_info(),
        ctx.accounts.token_config.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.extra_account_meta_list.to_account_info(),
        ctx.accounts.hook_program.to_account_info(),
    ];
    invoke_signed(&transfer_ix, &transfer_account_infos, signer_seed_groups).map_err(Into::into)
}

fn map_spl_error(_error: SplProgramError) -> anchor_lang::error::Error {
    anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData)
}
