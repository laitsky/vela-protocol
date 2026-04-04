use anchor_lang::{
    prelude::*,
    solana_program::{
        program::{invoke, invoke_signed},
        system_instruction,
    },
};
use solana_instruction::Instruction as SplInstruction;
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;
use spl_pod::optional_keys::OptionalNonZeroPubkey;
use spl_token_2022::{
    extension::{metadata_pointer, ExtensionType},
    instruction::{initialize_mint2, initialize_non_transferable_mint, initialize_permanent_delegate},
    state::Mint,
};
use spl_token_metadata_interface::{
    instruction::update_field,
    state::{Field, TokenMetadata},
};

use crate::{
    constants::{CREDENTIAL_DECIMALS, MIN_FREQUENCY_SECONDS},
    errors::VelaError,
    state::{MerchantState, PlanStatus, VelaPlan},
};

#[derive(Accounts)]
pub struct CreatePlan<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(
        init_if_needed,
        payer = merchant,
        space = MerchantState::SIZE,
        seeds = [MerchantState::SEED_PREFIX, merchant.key().as_ref()],
        bump
    )]
    pub merchant_state: Account<'info, MerchantState>,

    #[account(
        init,
        payer = merchant,
        space = VelaPlan::SIZE,
        seeds = [
            VelaPlan::SEED_PREFIX,
            merchant.key().as_ref(),
            merchant_state.plan_count.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub plan: Account<'info, VelaPlan>,

    #[account(
        mut,
        seeds = [
            b"credential",
            merchant.key().as_ref(),
            merchant_state.plan_count.to_le_bytes().as_ref()
        ],
        bump
    )]
    /// CHECK: PDA mint account is created via manual CPI and constrained by seeds.
    pub credential_mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: Address constraint ensures this is the Token-2022 program.
    pub token_2022_program: UncheckedAccount<'info>,

    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<CreatePlan>,
    amount: u64,
    frequency: u64,
    trial_period: u64,
    max_pulls: u64,
) -> Result<()> {
    require!(
        frequency >= MIN_FREQUENCY_SECONDS,
        VelaError::FrequencyTooLow
    );
    require!(max_pulls > 0, VelaError::MaxPullsTooLow);

    let token_2022_program_id = spl_token_2022::id();
    require_keys_eq!(
        ctx.accounts.token_2022_program.key(),
        anchor_pubkey(token_2022_program_id)
    );

    let merchant_key = ctx.accounts.merchant.key();
    let merchant_state = &mut ctx.accounts.merchant_state;
    let plan_id = merchant_state.plan_count;
    let plan_id_bytes = plan_id.to_le_bytes();
    let plan_key = ctx.accounts.plan.key();
    let credential_mint_key = ctx.accounts.credential_mint.key();

    if merchant_state.merchant == Pubkey::default() {
        merchant_state.merchant = merchant_key;
        merchant_state.bump = ctx.bumps.merchant_state;
    }

    let credential_name = format!("Vela Plan #{plan_id}");
    let credential_symbol = "VELA".to_string();
    let credential_uri = String::new();
    let additional_metadata = vec![
        ("plan_tier".to_string(), credential_name.clone()),
        ("plan_id".to_string(), plan_id.to_string()),
        (
            "subscription_start_source".to_string(),
            "VelaMandate.start_date".to_string(),
        ),
    ];
    let token_metadata = TokenMetadata {
        update_authority: OptionalNonZeroPubkey::try_from(Some(spl_pubkey(&plan_key)))
            .map_err(map_interface_error)?,
        mint: spl_pubkey(&credential_mint_key),
        name: credential_name.clone(),
        symbol: credential_symbol.clone(),
        uri: credential_uri.clone(),
        additional_metadata: additional_metadata.clone(),
    };

    let mint_extensions = [
        ExtensionType::NonTransferable,
        ExtensionType::MetadataPointer,
        ExtensionType::PermanentDelegate,
    ];
    let mint_len = ExtensionType::try_calculate_account_len::<Mint>(&mint_extensions)
        .map_err(map_interface_error)?;
    let funded_mint_len = mint_len
        .checked_add(token_metadata.tlv_size_of().map_err(map_interface_error)?)
        .ok_or(VelaError::Overflow)?;
    let mint_rent = ctx.accounts.rent.minimum_balance(funded_mint_len);

    let credential_signer_seeds: &[&[u8]] = &[
        b"credential",
        merchant_key.as_ref(),
        plan_id_bytes.as_ref(),
        &[ctx.bumps.credential_mint],
    ];
    let plan_signer_seeds: &[&[u8]] = &[
        VelaPlan::SEED_PREFIX,
        merchant_key.as_ref(),
        plan_id_bytes.as_ref(),
        &[ctx.bumps.plan],
    ];

    invoke_signed(
        &system_instruction::create_account(
            &merchant_key,
            &credential_mint_key,
            mint_rent,
            mint_len as u64,
            &ctx.accounts.token_2022_program.key(),
        ),
        &[
            ctx.accounts.merchant.to_account_info(),
            ctx.accounts.credential_mint.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[credential_signer_seeds],
    )?;

    invoke(
        &convert_instruction(
            initialize_non_transferable_mint(
                &token_2022_program_id,
                &spl_pubkey(&credential_mint_key),
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.credential_mint.to_account_info()],
    )?;

    // Initialize PermanentDelegate extension -- plan PDA is the permanent delegate,
    // enabling admin_cancel to burn credential tokens without subscriber consent.
    let init_permanent_delegate_ix = initialize_permanent_delegate(
        &token_2022_program_id,
        &spl_pubkey(&credential_mint_key),
        &spl_pubkey(&plan_key),
    )
    .map_err(map_interface_error)?;
    invoke(
        &convert_instruction(init_permanent_delegate_ix),
        &[ctx.accounts.credential_mint.to_account_info()],
    )?;

    invoke(
        &convert_instruction(
            metadata_pointer::instruction::initialize(
                &token_2022_program_id,
                &spl_pubkey(&credential_mint_key),
                Some(spl_pubkey(&plan_key)),
                Some(spl_pubkey(&credential_mint_key)),
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.credential_mint.to_account_info()],
    )?;

    invoke(
        &convert_instruction(
            initialize_mint2(
                &token_2022_program_id,
                &spl_pubkey(&credential_mint_key),
                &spl_pubkey(&plan_key),
                None,
                CREDENTIAL_DECIMALS,
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.credential_mint.to_account_info()],
    )?;

    let metadata_ix = convert_instruction(spl_token_metadata_interface::instruction::initialize(
        &token_2022_program_id,
        &spl_pubkey(&credential_mint_key),
        &spl_pubkey(&plan_key),
        &spl_pubkey(&credential_mint_key),
        &spl_pubkey(&plan_key),
        credential_name,
        credential_symbol,
        credential_uri,
    ));
    invoke_signed(
        &metadata_ix,
        &[
            ctx.accounts.credential_mint.to_account_info(),
            ctx.accounts.plan.to_account_info(),
            ctx.accounts.credential_mint.to_account_info(),
            ctx.accounts.plan.to_account_info(),
        ],
        &[plan_signer_seeds],
    )?;

    for (key, value) in additional_metadata {
        let update_ix = convert_instruction(update_field(
            &token_2022_program_id,
            &spl_pubkey(&credential_mint_key),
            &spl_pubkey(&plan_key),
            Field::Key(key),
            value,
        ));
        invoke_signed(
            &update_ix,
            &[
                ctx.accounts.token_2022_program.to_account_info(),
                ctx.accounts.credential_mint.to_account_info(),
                ctx.accounts.plan.to_account_info(),
            ],
            &[plan_signer_seeds],
        )?;
    }

    ctx.accounts.plan.set_inner(VelaPlan {
        merchant: merchant_key,
        plan_id,
        amount,
        frequency,
        trial_period,
        max_pulls,
        status: PlanStatus::Active,
        credential_mint: credential_mint_key,
        bump: ctx.bumps.plan,
    });

    merchant_state.plan_count = merchant_state
        .plan_count
        .checked_add(1)
        .ok_or(VelaError::Overflow)?;

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

fn map_interface_error(error: SplProgramError) -> anchor_lang::error::Error {
    let error = match error {
        SplProgramError::Custom(code) => ProgramError::Custom(code),
        SplProgramError::InvalidArgument => ProgramError::InvalidArgument,
        SplProgramError::InvalidInstructionData => ProgramError::InvalidInstructionData,
        SplProgramError::InvalidAccountData => ProgramError::InvalidAccountData,
        SplProgramError::AccountDataTooSmall => ProgramError::AccountDataTooSmall,
        SplProgramError::InsufficientFunds => ProgramError::InsufficientFunds,
        SplProgramError::IncorrectProgramId => ProgramError::IncorrectProgramId,
        SplProgramError::MissingRequiredSignature => ProgramError::MissingRequiredSignature,
        SplProgramError::AccountAlreadyInitialized => ProgramError::AccountAlreadyInitialized,
        SplProgramError::UninitializedAccount => ProgramError::UninitializedAccount,
        SplProgramError::NotEnoughAccountKeys => ProgramError::NotEnoughAccountKeys,
        SplProgramError::AccountBorrowFailed => ProgramError::AccountBorrowFailed,
        SplProgramError::MaxSeedLengthExceeded => ProgramError::MaxSeedLengthExceeded,
        SplProgramError::InvalidSeeds => ProgramError::InvalidSeeds,
        SplProgramError::BorshIoError => ProgramError::BorshIoError("borsh io error".into()),
        SplProgramError::AccountNotRentExempt => ProgramError::AccountNotRentExempt,
        SplProgramError::UnsupportedSysvar => ProgramError::UnsupportedSysvar,
        SplProgramError::IllegalOwner => ProgramError::IllegalOwner,
        SplProgramError::MaxAccountsDataAllocationsExceeded => {
            ProgramError::MaxAccountsDataAllocationsExceeded
        }
        SplProgramError::InvalidRealloc => ProgramError::InvalidRealloc,
        SplProgramError::MaxInstructionTraceLengthExceeded => {
            ProgramError::MaxInstructionTraceLengthExceeded
        }
        SplProgramError::BuiltinProgramsMustConsumeComputeUnits => {
            ProgramError::BuiltinProgramsMustConsumeComputeUnits
        }
        SplProgramError::InvalidAccountOwner => ProgramError::InvalidAccountOwner,
        SplProgramError::ArithmeticOverflow => ProgramError::ArithmeticOverflow,
        SplProgramError::Immutable => ProgramError::Immutable,
        SplProgramError::IncorrectAuthority => ProgramError::IncorrectAuthority,
    };
    error.into()
}
