use anchor_lang::prelude::*;
use anchor_spl::{
    token::{
        transfer_checked, Mint as SplMint, Token, TokenAccount as SplTokenAccount, TransferChecked,
    },
    token_2022::Token2022,
    token_interface::{burn, Burn, Mint, TokenAccount},
};

use crate::{
    constants::{AGENT_MANDATE_SEED, MINT_AUTHORITY_SEED, USDC_DECIMALS},
    errors::VelaError,
    instructions::agent_mandate_account::load_agent_mandate,
    state::{AgentMandateStatus, ProtocolConfig},
};

#[derive(Accounts)]
pub struct DrainAgentMandate<'info> {
    pub authority: Signer<'info>,

    /// CHECK: Used for PDA derivation only.
    pub agent: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [AGENT_MANDATE_SEED, authority.key().as_ref(), agent.key().as_ref()],
        bump,
    )]
    pub agent_mandate: UncheckedAccount<'info>,

    #[account(
        mut,
        token::mint = wrapped_usdc_mint,
        token::authority = agent_mandate,
        token::token_program = token_2022_program,
    )]
    pub mandate_wrapped_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = authority_usdc_account.owner == authority.key(),
        constraint = authority_usdc_account.mint == spl_usdc_mint.key() @ VelaError::UsdcMintMismatch,
    )]
    pub authority_usdc_account: Account<'info, SplTokenAccount>,

    #[account(
        mut,
        address = protocol_config.wrapped_usdc_mint,
    )]
    pub wrapped_usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    pub spl_usdc_mint: Account<'info, SplMint>,

    #[account(
        mut,
        address = protocol_config.wrapping_vault,
        constraint = wrapping_vault.mint == spl_usdc_mint.key() @ VelaError::UsdcMintMismatch,
        constraint = wrapping_vault.owner == mint_authority.key() @ VelaError::VaultMismatch,
    )]
    pub wrapping_vault: Account<'info, SplTokenAccount>,

    /// CHECK: PDA used as wrapping vault owner and wrapped mint authority.
    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    pub spl_token_program: Program<'info, Token>,
    pub token_2022_program: Program<'info, Token2022>,
}

pub fn handler(ctx: Context<DrainAgentMandate>) -> Result<()> {
    let loaded_mandate = load_agent_mandate(
        &ctx.accounts.agent_mandate.to_account_info(),
        &ctx.accounts.authority.key(),
        &ctx.accounts.agent.key(),
    )?;
    require_keys_eq!(
        loaded_mandate.authority(),
        ctx.accounts.authority.key(),
        VelaError::UnauthorizedAgentMandateAuthority
    );
    let mandate = loaded_mandate.into_current();
    let authority_key = ctx.accounts.authority.key();
    let agent_key = ctx.accounts.agent.key();
    let mandate_bump = [mandate.bump];
    let mandate_signer_seeds: &[&[u8]] = &[
        AGENT_MANDATE_SEED,
        authority_key.as_ref(),
        agent_key.as_ref(),
        &mandate_bump,
    ];
    let mandate_signer_seed_groups = [mandate_signer_seeds];

    let amount = ctx.accounts.mandate_wrapped_account.amount;
    if amount > 0 {
        let burn_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_2022_program.to_account_info(),
            Burn {
                mint: ctx.accounts.wrapped_usdc_mint.to_account_info(),
                from: ctx.accounts.mandate_wrapped_account.to_account_info(),
                authority: ctx.accounts.agent_mandate.to_account_info(),
            },
            &mandate_signer_seed_groups,
        );
        burn(burn_ctx, amount)?;

        let mint_authority_bump = ctx.bumps.mint_authority;
        let mint_authority_signer_seeds: &[&[u8]] = &[MINT_AUTHORITY_SEED, &[mint_authority_bump]];
        let mint_authority_signer_seed_groups = [mint_authority_signer_seeds];
        let transfer_ctx = CpiContext::new_with_signer(
            ctx.accounts.spl_token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.wrapping_vault.to_account_info(),
                mint: ctx.accounts.spl_usdc_mint.to_account_info(),
                to: ctx.accounts.authority_usdc_account.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            &mint_authority_signer_seed_groups,
        );
        transfer_checked(transfer_ctx, amount, USDC_DECIMALS)?;
    }

    ctx.accounts.mandate_wrapped_account.reload()?;
    emit!(AgentMandateDrained {
        mandate: ctx.accounts.agent_mandate.key(),
        authority: mandate.authority,
        agent: mandate.agent,
        status: mandate.status.clone(),
        daily_spent: mandate.daily_spent,
        total_spent: mandate.total_spent,
        remaining_balance: ctx.accounts.mandate_wrapped_account.amount,
    });

    Ok(())
}

#[event]
pub struct AgentMandateDrained {
    pub mandate: Pubkey,
    pub authority: Pubkey,
    pub agent: Pubkey,
    pub status: AgentMandateStatus,
    pub daily_spent: u64,
    pub total_spent: u64,
    pub remaining_balance: u64,
}
