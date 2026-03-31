use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint as SplMint, Token, TokenAccount as SplTokenAccount},
    token_2022::Token2022,
    token_interface::Mint,
};

use crate::{
    errors::VelaError,
    state::ProtocolConfig,
};

/// Admin-only instruction that creates the Token-2022 wrapped USDC mint.
///
/// Extensions initialized by Anchor's init constraints:
///   - TransferHook (authority = mint_authority, program_id = crate::ID)
///   - MetadataPointer (authority = mint_authority, metadata_address = self)
///   - PermanentDelegate (delegate = mint_authority) -- emergency-only (D-10)
///
/// TransferFee extension (D-08, D-09) is intentionally omitted at initialization.
/// Anchor 0.32.1 does not support `extensions::transfer_fee_config::*` constraints,
/// and Token-2022 requires all extensions to be initialized before InitializeMint2.
/// Since TransferFee is 0 bps at launch, omitting it is functionally equivalent.
/// If TransferFee is needed in the future, a separate program upgrade is required.
///
/// After mint creation, initializes the wrapping vault (SPL USDC ATA owned by mint_authority)
/// and stores both addresses in ProtocolConfig.
#[derive(Accounts)]
pub struct InitWrappedMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = config.bump,
        has_one = admin,
    )]
    pub config: Account<'info, ProtocolConfig>,

    /// CHECK: PDA used as mint authority, freeze authority, permanent delegate, and vault owner.
    /// Seeds: [b"mint-authority"]
    #[account(
        seeds = [b"mint-authority"],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    /// The Token-2022 wrapped USDC mint to initialize.
    /// Must be a signer (keypair generated client-side).
    #[account(
        init,
        signer,
        payer = admin,
        mint::token_program = token_2022_program,
        mint::decimals = 6,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority,
        extensions::transfer_hook::authority = mint_authority,
        extensions::transfer_hook::program_id = crate::ID,
        extensions::metadata_pointer::authority = mint_authority,
        extensions::metadata_pointer::metadata_address = wrapped_usdc_mint,
        extensions::permanent_delegate::delegate = mint_authority,
    )]
    pub wrapped_usdc_mint: Box<InterfaceAccount<'info, Mint>>,

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
}

pub fn handler(ctx: Context<InitWrappedMint>) -> Result<()> {
    // Ensure mint has not already been initialized (idempotency guard, D-03)
    require!(
        ctx.accounts.config.wrapped_usdc_mint == Pubkey::default(),
        VelaError::WrappedMintAlreadyInitialized
    );

    // Store mint and vault addresses in ProtocolConfig
    ctx.accounts.config.wrapped_usdc_mint = ctx.accounts.wrapped_usdc_mint.key();
    ctx.accounts.config.wrapping_vault = ctx.accounts.wrapping_vault.key();

    Ok(())
}
