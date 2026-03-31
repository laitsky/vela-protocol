use anchor_lang::prelude::*;
use anchor_spl::{
    token::{transfer_checked, Mint as SplMint, Token, TokenAccount as SplTokenAccount, TransferChecked},
    token_2022::Token2022,
    token_interface::{mint_to, Mint, MintTo, TokenAccount},
};

use crate::{constants::USDC_DECIMALS, errors::VelaError, state::ProtocolConfig};

#[derive(Accounts)]
pub struct Wrap<'info> {
    pub subscriber: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = config.bump,
    )]
    pub config: Account<'info, ProtocolConfig>,

    /// The SPL USDC mint (Token program, not Token-2022).
    pub spl_usdc_mint: Account<'info, SplMint>,

    /// The Token-2022 wrapped USDC mint (must match config.wrapped_usdc_mint).
    #[account(
        mut,
        address = config.wrapped_usdc_mint,
    )]
    pub wrapped_usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Subscriber's SPL USDC token account (source of SPL USDC deposit).
    #[account(
        mut,
        constraint = subscriber_usdc_account.owner == subscriber.key(),
        constraint = subscriber_usdc_account.mint == spl_usdc_mint.key(),
    )]
    pub subscriber_usdc_account: Account<'info, SplTokenAccount>,

    /// Subscriber's Token-2022 wrapped USDC account (destination for minted wrapped USDC).
    #[account(
        mut,
        token::mint = wrapped_usdc_mint,
        token::authority = subscriber,
        token::token_program = token_2022_program,
    )]
    pub subscriber_wrapped_account: InterfaceAccount<'info, TokenAccount>,

    /// Protocol's SPL USDC vault (receives subscriber deposit, must match config.wrapping_vault).
    #[account(
        mut,
        address = config.wrapping_vault,
    )]
    pub wrapping_vault: Account<'info, SplTokenAccount>,

    /// CHECK: PDA used as mint authority and vault owner. Seeds: [b"mint-authority"]
    #[account(
        seeds = [b"mint-authority"],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    pub spl_token_program: Program<'info, Token>,
    pub token_2022_program: Program<'info, Token2022>,
}

pub fn handler(ctx: Context<Wrap>, amount: u64) -> Result<()> {
    // Validate that wrapped mint has been initialized (D-03)
    require!(
        ctx.accounts.config.wrapped_usdc_mint != Pubkey::default(),
        VelaError::WrappedMintNotInitialized
    );

    // Step 1: Transfer SPL USDC from subscriber to the protocol vault.
    let transfer_ctx = CpiContext::new(
        ctx.accounts.spl_token_program.to_account_info(),
        TransferChecked {
            from: ctx.accounts.subscriber_usdc_account.to_account_info(),
            mint: ctx.accounts.spl_usdc_mint.to_account_info(),
            to: ctx.accounts.wrapping_vault.to_account_info(),
            authority: ctx.accounts.subscriber.to_account_info(),
        },
    );
    transfer_checked(transfer_ctx, amount, USDC_DECIMALS)?;

    // Step 2: Mint equivalent wrapped USDC to subscriber's Token-2022 account.
    // Signed by mint_authority PDA.
    let mint_authority_bump = ctx.bumps.mint_authority;
    let signer_seeds: &[&[u8]] = &[b"mint-authority", &[mint_authority_bump]];
    let signer_seed_groups = [signer_seeds];

    let mint_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_2022_program.to_account_info(),
        MintTo {
            mint: ctx.accounts.wrapped_usdc_mint.to_account_info(),
            to: ctx.accounts.subscriber_wrapped_account.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        },
        &signer_seed_groups,
    );
    mint_to(mint_ctx, amount)?;

    Ok(())
}
