use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{
        transfer_checked, Mint as SplMint, Token, TokenAccount as SplTokenAccount, TransferChecked,
    },
    token_2022::Token2022,
    token_interface::{mint_to, Mint, MintTo, TokenAccount},
};

use crate::{
    constants::{AGENT_MANDATE_SEED, MINT_AUTHORITY_SEED, USDC_DECIMALS},
    errors::VelaError,
    instructions::agent_mandate_account::{initialize_agent_mandate, load_agent_mandate},
    state::{AgentMandate, AgentMandateStatus, ProtocolConfig, ServiceLimit},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ServiceLimitInput {
    pub service: Pubkey,
    pub daily_limit: u64,
}

#[derive(Accounts)]
pub struct CreateAgentMandate<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Used for PDA derivation and persisted mandate binding only.
    pub agent: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [AGENT_MANDATE_SEED, authority.key().as_ref(), agent.key().as_ref()],
        bump,
    )]
    pub agent_mandate: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = authority_usdc_account.owner == authority.key(),
        constraint = authority_usdc_account.mint == spl_usdc_mint.key() @ VelaError::UsdcMintMismatch,
    )]
    pub authority_usdc_account: Account<'info, SplTokenAccount>,

    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = wrapped_usdc_mint,
        associated_token::authority = agent_mandate,
        associated_token::token_program = token_2022_program,
    )]
    pub mandate_wrapped_account: InterfaceAccount<'info, TokenAccount>,

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

    /// The SPL USDC mint backing the protocol wrapping vault.
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
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handler(
    ctx: Context<CreateAgentMandate>,
    daily_limit: u64,
    lifetime_cap: u64,
    min_pull_amount: u64,
    min_pull_interval: i64,
    services: Vec<ServiceLimitInput>,
    funded_amount: u64,
) -> Result<()> {
    require!(
        services.len() <= AgentMandate::MAX_SERVICES,
        VelaError::TooManyServices
    );
    require!(min_pull_interval >= 0, VelaError::InvalidFrequency);

    for i in 0..services.len() {
        for j in (i + 1)..services.len() {
            require!(
                services[i].service != services[j].service,
                VelaError::DuplicateService
            );
        }
    }

    let now = Clock::get()?.unix_timestamp;
    let mapped_services: Vec<ServiceLimit> = services
        .iter()
        .map(|entry| ServiceLimit {
            service: entry.service,
            daily_limit: entry.daily_limit,
            daily_spent: 0,
            last_reset: now,
        })
        .collect();

    if !ctx.accounts.agent_mandate.data_is_empty() {
        let _existing = load_agent_mandate(
            &ctx.accounts.agent_mandate.to_account_info(),
            &ctx.accounts.authority.key(),
            &ctx.accounts.agent.key(),
        )?;
        return Err(VelaError::AgentMandateAlreadyExists.into());
    }

    let mandate = AgentMandate::new(
        ctx.accounts.authority.key(),
        ctx.accounts.agent.key(),
        daily_limit,
        0,
        now,
        lifetime_cap,
        0,
        min_pull_amount,
        min_pull_interval,
        0,
        AgentMandateStatus::Active,
        mapped_services,
        ctx.bumps.agent_mandate,
    );
    initialize_agent_mandate(
        &ctx.accounts.authority.to_account_info(),
        &ctx.accounts.agent_mandate.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.authority.key(),
        &ctx.accounts.agent.key(),
        &mandate,
    )?;

    let transfer_ctx = CpiContext::new(
        ctx.accounts.spl_token_program.to_account_info(),
        TransferChecked {
            from: ctx.accounts.authority_usdc_account.to_account_info(),
            mint: ctx.accounts.spl_usdc_mint.to_account_info(),
            to: ctx.accounts.wrapping_vault.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        },
    );
    transfer_checked(transfer_ctx, funded_amount, USDC_DECIMALS)?;

    let mint_authority_bump = ctx.bumps.mint_authority;
    let signer_seeds: &[&[u8]] = &[MINT_AUTHORITY_SEED, &[mint_authority_bump]];
    let signer_seed_groups = [signer_seeds];
    let mint_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_2022_program.to_account_info(),
        MintTo {
            mint: ctx.accounts.wrapped_usdc_mint.to_account_info(),
            to: ctx.accounts.mandate_wrapped_account.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        },
        &signer_seed_groups,
    );
    mint_to(mint_ctx, funded_amount)?;
    ctx.accounts.mandate_wrapped_account.reload()?;

    emit!(AgentMandateCreated {
        mandate: ctx.accounts.agent_mandate.key(),
        authority: ctx.accounts.authority.key(),
        agent: ctx.accounts.agent.key(),
        daily_limit,
        lifetime_cap,
        service_count: mandate.services.len() as u8,
        funded_amount,
        remaining_balance: ctx.accounts.mandate_wrapped_account.amount,
    });

    Ok(())
}

#[event]
pub struct AgentMandateCreated {
    pub mandate: Pubkey,
    pub authority: Pubkey,
    pub agent: Pubkey,
    pub daily_limit: u64,
    pub lifetime_cap: u64,
    pub service_count: u8,
    pub funded_amount: u64,
    pub remaining_balance: u64,
}
