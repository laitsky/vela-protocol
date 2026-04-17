use anchor_lang::prelude::*;
use anchor_spl::{
    token::{
        transfer_checked, Mint as SplMint, Token, TokenAccount as SplTokenAccount, TransferChecked,
    },
    token_2022::Token2022,
    token_interface::{burn, Burn, Mint, TokenAccount},
};

use crate::{
    constants::USDC_DECIMALS, errors::VelaError,
    instructions::protocol_config_account::load_protocol_config, state::ProtocolConfig,
};

#[derive(Accounts)]
pub struct Unwrap<'info> {
    pub user: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: UncheckedAccount<'info>,

    /// The SPL USDC mint (Token program, not Token-2022).
    pub spl_usdc_mint: Account<'info, SplMint>,

    /// The Token-2022 wrapped USDC mint (must match config.wrapped_usdc_mint).
    /// CHECK: Validated in handler against loaded protocol_config.wrapped_usdc_mint.
    #[account(mut)]
    pub wrapped_usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    /// User's Token-2022 wrapped USDC account (source to burn from).
    #[account(
        mut,
        token::mint = wrapped_usdc_mint,
        token::authority = user,
        token::token_program = token_2022_program,
    )]
    pub user_wrapped_account: InterfaceAccount<'info, TokenAccount>,

    /// User's SPL USDC account (destination for released USDC).
    #[account(
        mut,
        constraint = user_usdc_account.owner == user.key(),
        constraint = user_usdc_account.mint == spl_usdc_mint.key(),
    )]
    pub user_usdc_account: Account<'info, SplTokenAccount>,

    /// Protocol's SPL USDC vault (releases USDC to user, must match config.wrapping_vault).
    /// CHECK: Validated in handler against loaded protocol_config.wrapping_vault.
    #[account(mut)]
    pub wrapping_vault: Account<'info, SplTokenAccount>,

    /// CHECK: PDA used as vault authority and mint authority. Seeds: [b"mint-authority"]
    #[account(
        seeds = [b"mint-authority"],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    pub spl_token_program: Program<'info, Token>,
    pub token_2022_program: Program<'info, Token2022>,
}

pub fn handler(ctx: Context<Unwrap>, amount: u64) -> Result<()> {
    let config = load_protocol_config(&ctx.accounts.config.to_account_info())?;
    require_keys_eq!(
        ctx.accounts.wrapped_usdc_mint.key(),
        config.wrapped_usdc_mint(),
        VelaError::UsdcMintMismatch
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.key(),
        config.wrapping_vault(),
        VelaError::VaultMismatch
    );

    // Step 1: Burn wrapped USDC from user's Token-2022 account.
    // User is the authority for their own token account.
    let burn_ctx = CpiContext::new(
        ctx.accounts.token_2022_program.to_account_info(),
        Burn {
            mint: ctx.accounts.wrapped_usdc_mint.to_account_info(),
            from: ctx.accounts.user_wrapped_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    );
    burn(burn_ctx, amount)?;

    // Step 2: Transfer equivalent SPL USDC from vault to user's SPL USDC account.
    // Signed by mint_authority PDA (which owns the vault).
    let mint_authority_bump = ctx.bumps.mint_authority;
    let signer_seeds: &[&[u8]] = &[b"mint-authority", &[mint_authority_bump]];
    let signer_seed_groups = [signer_seeds];

    let transfer_ctx = CpiContext::new_with_signer(
        ctx.accounts.spl_token_program.to_account_info(),
        TransferChecked {
            from: ctx.accounts.wrapping_vault.to_account_info(),
            mint: ctx.accounts.spl_usdc_mint.to_account_info(),
            to: ctx.accounts.user_usdc_account.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        },
        &signer_seed_groups,
    );
    transfer_checked(transfer_ctx, amount, USDC_DECIMALS)?;

    Ok(())
}
