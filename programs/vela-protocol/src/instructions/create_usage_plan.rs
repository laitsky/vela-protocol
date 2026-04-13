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
    instruction::{initialize_mint2, initialize_non_transferable_mint},
    state::Mint,
};
use spl_token_metadata_interface::{
    instruction::update_field,
    state::{Field, TokenMetadata},
};

use crate::{
    constants::CREDENTIAL_DECIMALS,
    errors::VelaError,
    instructions::merchant_account::ensure_merchant_state,
    state::{PlanStatus, PricingTier, UsagePlan},
};

#[derive(Accounts)]
#[instruction(plan_id: u64)]
pub struct CreateUsagePlan<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(mut, seeds = [crate::state::MerchantState::SEED_PREFIX, merchant.key().as_ref()], bump)]
    pub merchant_state: UncheckedAccount<'info>,

    #[account(
        init,
        payer = merchant,
        space = UsagePlan::SIZE,
        seeds = [
            UsagePlan::SEED_PREFIX,
            merchant.key().as_ref(),
            plan_id.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub usage_plan: Account<'info, UsagePlan>,

    #[account(
        mut,
        seeds = [
            b"usage_credential",
            merchant.key().as_ref(),
            plan_id.to_le_bytes().as_ref()
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
    ctx: Context<CreateUsagePlan>,
    plan_id: u64,
    unit_name: [u8; 32],
    tiers: Vec<PricingTier>,
    max_charge_per_period: u64,
    settlement_frequency: u64,
) -> Result<()> {
    // Validate tier count: must be 1-5
    require!(
        tiers.len() >= 1 && tiers.len() <= 5,
        VelaError::InvalidTierCount
    );

    // Validate max charge and settlement frequency
    require!(max_charge_per_period > 0, VelaError::InvalidAmount);
    require!(settlement_frequency > 0, VelaError::InvalidFrequency);

    // Validate tier boundaries are monotonically increasing (except last which can be 0 = unlimited)
    for i in 1..tiers.len() {
        let prev = &tiers[i - 1];
        let curr = &tiers[i];
        // Intermediate tiers must have non-zero up_to, and each must be greater than the previous
        if i < tiers.len() - 1 {
            require!(curr.up_to > prev.up_to, VelaError::InvalidTierBoundary);
        } else {
            // Last tier: up_to == 0 means unlimited; if non-zero, must be > previous
            if curr.up_to != 0 {
                require!(curr.up_to > prev.up_to, VelaError::InvalidTierBoundary);
            }
        }
    }

    let token_2022_program_id = spl_token_2022::id();
    require_keys_eq!(
        ctx.accounts.token_2022_program.key(),
        anchor_pubkey(token_2022_program_id)
    );

    let merchant_key = ctx.accounts.merchant.key();
    ensure_merchant_state(
        &ctx.accounts.merchant.to_account_info(),
        &ctx.accounts.merchant_state.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.rent,
        &merchant_key,
        ctx.bumps.merchant_state,
    )?;
    let plan_id_bytes = plan_id.to_le_bytes();
    let usage_plan_key = ctx.accounts.usage_plan.key();
    let credential_mint_key = ctx.accounts.credential_mint.key();

    // Build unit name string for metadata (trim null bytes)
    let unit_name_str = std::str::from_utf8(&unit_name)
        .unwrap_or("units")
        .trim_end_matches('\0');
    let credential_name = format!("Vela Usage Plan #{plan_id} ({unit_name_str})");
    let credential_symbol = "VELA-U".to_string();
    let credential_uri = String::new();
    let additional_metadata = vec![
        ("plan_type".to_string(), "usage".to_string()),
        ("plan_id".to_string(), plan_id.to_string()),
        ("unit_name".to_string(), unit_name_str.to_string()),
    ];
    let token_metadata = TokenMetadata {
        update_authority: OptionalNonZeroPubkey::try_from(Some(spl_pubkey(&usage_plan_key)))
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
    ];
    let mint_len = ExtensionType::try_calculate_account_len::<Mint>(&mint_extensions)
        .map_err(map_interface_error)?;
    let funded_mint_len = mint_len
        .checked_add(token_metadata.tlv_size_of().map_err(map_interface_error)?)
        .ok_or(VelaError::Overflow)?;
    let mint_rent = ctx.accounts.rent.minimum_balance(funded_mint_len);

    let credential_signer_seeds: &[&[u8]] = &[
        b"usage_credential",
        merchant_key.as_ref(),
        plan_id_bytes.as_ref(),
        &[ctx.bumps.credential_mint],
    ];
    let usage_plan_signer_seeds: &[&[u8]] = &[
        UsagePlan::SEED_PREFIX,
        merchant_key.as_ref(),
        plan_id_bytes.as_ref(),
        &[ctx.bumps.usage_plan],
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

    invoke(
        &convert_instruction(
            metadata_pointer::instruction::initialize(
                &token_2022_program_id,
                &spl_pubkey(&credential_mint_key),
                Some(spl_pubkey(&usage_plan_key)),
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
                &spl_pubkey(&usage_plan_key),
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
        &spl_pubkey(&usage_plan_key),
        &spl_pubkey(&credential_mint_key),
        &spl_pubkey(&usage_plan_key),
        credential_name,
        credential_symbol,
        credential_uri,
    ));
    invoke_signed(
        &metadata_ix,
        &[
            ctx.accounts.credential_mint.to_account_info(),
            ctx.accounts.usage_plan.to_account_info(),
            ctx.accounts.credential_mint.to_account_info(),
            ctx.accounts.usage_plan.to_account_info(),
        ],
        &[usage_plan_signer_seeds],
    )?;

    for (key, value) in additional_metadata {
        let update_ix = convert_instruction(update_field(
            &token_2022_program_id,
            &spl_pubkey(&credential_mint_key),
            &spl_pubkey(&usage_plan_key),
            Field::Key(key),
            value,
        ));
        invoke_signed(
            &update_ix,
            &[
                ctx.accounts.token_2022_program.to_account_info(),
                ctx.accounts.credential_mint.to_account_info(),
                ctx.accounts.usage_plan.to_account_info(),
            ],
            &[usage_plan_signer_seeds],
        )?;
    }

    // Copy tiers into fixed array, zero-fill unused slots
    let mut tiers_array = [PricingTier::default(); 5];
    for (i, tier) in tiers.iter().enumerate() {
        tiers_array[i] = *tier;
    }

    ctx.accounts.usage_plan.set_inner(UsagePlan {
        merchant: merchant_key,
        plan_id,
        unit_name,
        tiers: tiers_array,
        tier_count: tiers.len() as u8,
        max_charge_per_period,
        settlement_frequency,
        credential_mint: credential_mint_key,
        status: PlanStatus::Active,
        bump: ctx.bumps.usage_plan,
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
