use anchor_lang::{
    prelude::*,
    solana_program::{
        program::{invoke, invoke_signed},
        program_error::ProgramError,
        system_instruction,
    },
};
use solana_instruction::Instruction as SplInstruction;
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;
use spl_token_2022::{
    extension::ExtensionType,
    instruction::{initialize_mint2, initialize_non_transferable_mint, initialize_permanent_delegate},
    state::Mint,
};

use crate::{
    constants::CREDENTIAL_DECIMALS,
    instructions::merchant_account::{ensure_merchant_state, write_merchant_state},
    state::MerchantState,
};

#[derive(Accounts)]
pub struct InitMerchantCredential<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(mut, seeds = [MerchantState::SEED_PREFIX, merchant.key().as_ref()], bump)]
    pub merchant_state: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: PDA mint account created via manual CPI, constrained by seeds.
    pub credential_mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: Address constraint ensures this is the Token-2022 program.
    pub token_2022_program: UncheckedAccount<'info>,

    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<InitMerchantCredential>) -> Result<()> {
    let token_2022_program_id = spl_token_2022::id();
    require_keys_eq!(
        ctx.accounts.token_2022_program.key(),
        anchor_pubkey(token_2022_program_id)
    );

    let merchant_key = ctx.accounts.merchant.key();
    let credential_mint_key = ctx.accounts.credential_mint.key();

    // Validate credential mint PDA: seeds = ["merchant-credential", merchant]
    let (expected_credential_key, credential_bump) = Pubkey::find_program_address(
        &[b"merchant-credential", merchant_key.as_ref()],
        &crate::ID,
    );
    if credential_mint_key != expected_credential_key {
        return Err(ProgramError::InvalidSeeds.into());
    }

    let merchant_state_info = ctx.accounts.merchant_state.to_account_info();
    let mut merchant_state = ensure_merchant_state(
        &ctx.accounts.merchant.to_account_info(),
        &merchant_state_info,
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.rent,
        &merchant_key,
        ctx.bumps.merchant_state,
    )?;

    // Idempotent: if credential_mint is already set and matches, succeed without side effects
    if merchant_state.credential_mint != Pubkey::default() {
        if merchant_state.credential_mint == credential_mint_key {
            return Ok(());
        }
        // A different mint is already set — this should not happen but guard against it
        return Err(ProgramError::AccountAlreadyInitialized.into());
    }

    // Create the credential mint (non-transferable, merchant-state as mint authority)

    let mint_extensions = [
        ExtensionType::NonTransferable,
        ExtensionType::PermanentDelegate,
    ];
    let mint_len = ExtensionType::try_calculate_account_len::<Mint>(&mint_extensions)
        .map_err(map_interface_error)?;
    let mint_rent = ctx.accounts.rent.minimum_balance(mint_len);

    let credential_signer_seeds: &[&[u8]] = &[
        b"merchant-credential",
        merchant_key.as_ref(),
        &[credential_bump],
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

    // Initialize non-transferable extension
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

    // Initialize permanent delegate -- MerchantState PDA is the permanent delegate
    // so that cancel/admin_cancel can burn credentials without subscriber consent
    let merchant_state_spl = spl_pubkey(merchant_state_info.key);
    let init_permanent_delegate_ix = initialize_permanent_delegate(
        &token_2022_program_id,
        &spl_pubkey(&credential_mint_key),
        &merchant_state_spl,
    )
    .map_err(map_interface_error)?;
    invoke(
        &convert_instruction(init_permanent_delegate_ix),
        &[ctx.accounts.credential_mint.to_account_info()],
    )?;

    // Initialize mint2 with MerchantState PDA as mint authority
    invoke(
        &convert_instruction(
            initialize_mint2(
                &token_2022_program_id,
                &spl_pubkey(&credential_mint_key),
                &merchant_state_spl,
                None,
                CREDENTIAL_DECIMALS,
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.credential_mint.to_account_info()],
    )?;

    // Store credential mint on MerchantState
    merchant_state.credential_mint = credential_mint_key;
    write_merchant_state(&merchant_state_info, &merchant_state)?;

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
