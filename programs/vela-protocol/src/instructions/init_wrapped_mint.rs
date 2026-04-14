use anchor_lang::{
    prelude::*,
    solana_program::{
        program::{invoke, invoke_signed},
        system_instruction,
    },
};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint as SplMint, Token, TokenAccount as SplTokenAccount},
    token_2022::Token2022,
};
use solana_instruction::Instruction as SplInstruction;
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;
use spl_pod::optional_keys::OptionalNonZeroPubkey;
use spl_token_2022::{
    extension::ExtensionType,
    instruction::initialize_mint2,
    state::Mint as Token2022Mint,
};
use spl_token_metadata_interface::state::TokenMetadata;

use crate::{
    constants::{
        TRANSFER_FEE_BASIS_POINTS, TRANSFER_FEE_MAXIMUM, TRANSFER_HOOK_PROGRAM_ID, USDC_DECIMALS,
        WRAPPED_USDC_NAME, WRAPPED_USDC_SYMBOL, WRAPPED_USDC_URI,
    },
    errors::VelaError,
    instructions::protocol_config_account::{
        load_protocol_config, upgrade_protocol_config, write_protocol_config,
    },
    state::ProtocolConfig,
};

/// Admin-only instruction that creates the Token-2022 wrapped USDC mint.
///
/// The mint follows the standard Token-2022 initialization flow:
/// allocate space for the fixed extensions, initialize the mint, then let the
/// token-metadata instruction realloc the account for variable-length metadata.
#[derive(Accounts)]
pub struct InitWrappedMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: UncheckedAccount<'info>,

    /// CHECK: PDA used as mint authority, freeze authority, update authority,
    /// permanent delegate, and vault owner. Seeds: [b"mint-authority"]
    #[account(
        seeds = [b"mint-authority"],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    /// The Token-2022 wrapped USDC mint account to create.
    #[account(mut)]
    pub wrapped_usdc_mint: Signer<'info>,

    /// The SPL USDC mint (not Token-2022) used for the backing vault.
    pub spl_usdc_mint: Account<'info, SplMint>,

    /// The SPL USDC vault ATA for holding subscriber deposits.
    /// Owned by mint_authority PDA, associated with spl_usdc_mint.
    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = spl_usdc_mint,
        associated_token::authority = mint_authority,
        associated_token::token_program = spl_token_program,
    )]
    pub wrapping_vault: Account<'info, SplTokenAccount>,

    pub token_2022_program: Program<'info, Token2022>,
    pub spl_token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<InitWrappedMint>) -> Result<()> {
    let config_info = ctx.accounts.config.to_account_info();
    let existing_config = load_protocol_config(&config_info)?;
    require_keys_eq!(
        existing_config.admin(),
        ctx.accounts.admin.key(),
        VelaError::UnauthorizedAdmin
    );
    let mut config = upgrade_protocol_config(
        &ctx.accounts.admin.to_account_info(),
        &config_info,
        &ctx.accounts.system_program.to_account_info(),
    )?;
    require!(
        config.wrapped_usdc_mint == Pubkey::default(),
        VelaError::WrappedMintAlreadyInitialized
    );

    let mint_key = ctx.accounts.wrapped_usdc_mint.key();
    let mint_authority_key = ctx.accounts.mint_authority.key();
    let token_2022_program_id = spl_token_2022::id();

    let token_metadata = TokenMetadata {
        update_authority: OptionalNonZeroPubkey::try_from(Some(spl_pubkey(&mint_authority_key)))
            .map_err(map_interface_error)?,
        mint: spl_pubkey(&mint_key),
        name: WRAPPED_USDC_NAME.to_string(),
        symbol: WRAPPED_USDC_SYMBOL.to_string(),
        uri: WRAPPED_USDC_URI.to_string(),
        additional_metadata: Vec::new(),
    };

    let mint_extensions = [
        ExtensionType::TransferHook,
        ExtensionType::TransferFeeConfig,
        ExtensionType::MetadataPointer,
        ExtensionType::PermanentDelegate,
    ];
    let mint_len = ExtensionType::try_calculate_account_len::<Token2022Mint>(&mint_extensions)
        .map_err(map_interface_error)?;
    let funded_mint_len = mint_len
        .checked_add(token_metadata.tlv_size_of().map_err(map_interface_error)?)
        .ok_or(VelaError::Overflow)?;
    let mint_rent = ctx.accounts.rent.minimum_balance(funded_mint_len);

    invoke(
        &system_instruction::create_account(
            &ctx.accounts.admin.key(),
            &mint_key,
            mint_rent,
            mint_len as u64,
            &ctx.accounts.token_2022_program.key(),
        ),
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.wrapped_usdc_mint.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;

    invoke(
        &convert_instruction(
            spl_token_2022::extension::transfer_hook::instruction::initialize(
                &token_2022_program_id,
                &spl_pubkey(&mint_key),
                Some(spl_pubkey(&mint_authority_key)),
                Some(spl_pubkey(&TRANSFER_HOOK_PROGRAM_ID)),
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.wrapped_usdc_mint.to_account_info()],
    )?;

    let transfer_fee_authority = spl_pubkey(&mint_authority_key);
    invoke(
        &convert_instruction(
            spl_token_2022::extension::transfer_fee::instruction::initialize_transfer_fee_config(
                &token_2022_program_id,
                &spl_pubkey(&mint_key),
                Some(&transfer_fee_authority),
                Some(&transfer_fee_authority),
                TRANSFER_FEE_BASIS_POINTS,
                TRANSFER_FEE_MAXIMUM,
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.wrapped_usdc_mint.to_account_info()],
    )?;

    invoke(
        &convert_instruction(
            spl_token_2022::extension::metadata_pointer::instruction::initialize(
                &token_2022_program_id,
                &spl_pubkey(&mint_key),
                Some(spl_pubkey(&mint_authority_key)),
                Some(spl_pubkey(&mint_key)),
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.wrapped_usdc_mint.to_account_info()],
    )?;

    invoke(
        &convert_instruction(
            spl_token_2022::instruction::initialize_permanent_delegate(
                &token_2022_program_id,
                &spl_pubkey(&mint_key),
                &spl_pubkey(&mint_authority_key),
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.wrapped_usdc_mint.to_account_info()],
    )?;

    invoke(
        &convert_instruction(
            initialize_mint2(
                &token_2022_program_id,
                &spl_pubkey(&mint_key),
                &spl_pubkey(&mint_authority_key),
                Some(&spl_pubkey(&mint_authority_key)),
                USDC_DECIMALS,
            )
            .map_err(map_interface_error)?,
        ),
        &[ctx.accounts.wrapped_usdc_mint.to_account_info()],
    )?;

    let mint_authority_bump = ctx.bumps.mint_authority;
    let mint_authority_signer_seeds: &[&[u8]] = &[b"mint-authority", &[mint_authority_bump]];

    let metadata_ix = convert_instruction(spl_token_metadata_interface::instruction::initialize(
        &token_2022_program_id,
        &spl_pubkey(&mint_key),
        &spl_pubkey(&mint_authority_key),
        &spl_pubkey(&mint_key),
        &spl_pubkey(&mint_authority_key),
        WRAPPED_USDC_NAME.to_string(),
        WRAPPED_USDC_SYMBOL.to_string(),
        WRAPPED_USDC_URI.to_string(),
    ));
    invoke_signed(
        &metadata_ix,
        &[
            ctx.accounts.wrapped_usdc_mint.to_account_info(),
            ctx.accounts.mint_authority.to_account_info(),
            ctx.accounts.wrapped_usdc_mint.to_account_info(),
            ctx.accounts.mint_authority.to_account_info(),
        ],
        &[mint_authority_signer_seeds],
    )?;

    require_keys_eq!(
        ctx.accounts.wrapping_vault.mint,
        ctx.accounts.spl_usdc_mint.key(),
        VelaError::UsdcMintMismatch
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.owner,
        ctx.accounts.mint_authority.key(),
        VelaError::VaultMismatch
    );

    config.wrapped_usdc_mint = mint_key;
    config.wrapping_vault = ctx.accounts.wrapping_vault.key();
    write_protocol_config(&config_info, &config)?;

    Ok(())
}

fn spl_pubkey(key: &Pubkey) -> SplPubkey {
    SplPubkey::from(key.to_bytes())
}

fn anchor_pubkey(key: SplPubkey) -> Pubkey {
    Pubkey::new_from_array(key.to_bytes())
}

fn convert_instruction(ix: SplInstruction) -> anchor_lang::solana_program::instruction::Instruction {
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

fn map_interface_error(_error: SplProgramError) -> anchor_lang::error::Error {
    anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData)
}
