use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::Token2022,
    token_interface::{Mint, TokenAccount},
};

use crate::{
    constants::AGENT_MANDATE_SEED,
    errors::VelaError,
    instructions::agent_mandate_account::{load_agent_mandate, write_agent_mandate},
    instructions::ServiceLimitInput,
    state::{AgentMandate, AgentMandateStatus, ProtocolConfig, ServiceLimit},
};

#[derive(Accounts)]
pub struct AdjustAgentMandate<'info> {
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
        token::mint = wrapped_usdc_mint,
        token::authority = agent_mandate,
        token::token_program = token_2022_program,
    )]
    pub mandate_wrapped_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        address = protocol_config.wrapped_usdc_mint,
    )]
    pub wrapped_usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    pub token_2022_program: Program<'info, Token2022>,
}

#[allow(clippy::too_many_arguments)]
pub fn handler(
    ctx: Context<AdjustAgentMandate>,
    daily_limit: Option<u64>,
    lifetime_cap: Option<u64>,
    min_pull_amount: Option<u64>,
    min_pull_interval: Option<i64>,
    services: Option<Vec<ServiceLimitInput>>,
) -> Result<()> {
    require!(
        daily_limit.is_some()
            || lifetime_cap.is_some()
            || min_pull_amount.is_some()
            || min_pull_interval.is_some()
            || services.is_some(),
        VelaError::NoUpdateProvided
    );

    let mandate_info = ctx.accounts.agent_mandate.to_account_info();
    let loaded_mandate = load_agent_mandate(
        &mandate_info,
        &ctx.accounts.authority.key(),
        &ctx.accounts.agent.key(),
    )?;
    require_keys_eq!(
        loaded_mandate.authority(),
        ctx.accounts.authority.key(),
        VelaError::UnauthorizedAgentMandateAuthority
    );
    let legacy_layout = loaded_mandate.is_legacy();
    let mut mandate = loaded_mandate.into_current();
    match mandate.status {
        AgentMandateStatus::Active | AgentMandateStatus::Paused => {}
        AgentMandateStatus::Revoked => {
            return Err(VelaError::InvalidAgentMandateStatusTransition.into());
        }
    }

    if let Some(next_daily_limit) = daily_limit {
        require!(next_daily_limit >= mandate.daily_spent, VelaError::InvalidAmount);
        mandate.daily_limit = next_daily_limit;
    }
    if let Some(next_lifetime_cap) = lifetime_cap {
        require!(
            next_lifetime_cap >= mandate.total_spent,
            VelaError::InvalidAmount
        );
        mandate.lifetime_cap = next_lifetime_cap;
    }
    if let Some(next_min_pull_amount) = min_pull_amount {
        mandate.min_pull_amount = next_min_pull_amount;
    }
    if let Some(next_min_pull_interval) = min_pull_interval {
        require!(next_min_pull_interval >= 0, VelaError::InvalidFrequency);
        mandate.min_pull_interval = next_min_pull_interval;
    }

    if let Some(next_services) = services {
        require!(
            next_services.len() <= AgentMandate::MAX_SERVICES,
            VelaError::TooManyServices
        );
        for i in 0..next_services.len() {
            for j in (i + 1)..next_services.len() {
                require!(
                    next_services[i].service != next_services[j].service,
                    VelaError::DuplicateService
                );
            }
        }

        let now = Clock::get()?.unix_timestamp;
        let current_services = mandate.services.clone();
        let updated_services: Vec<ServiceLimit> = next_services
            .into_iter()
            .map(|service_input| {
                let existing = current_services
                    .iter()
                    .find(|service_limit| service_limit.service == service_input.service);
                let (daily_spent, last_reset) = existing
                    .map(|service_limit| (service_limit.daily_spent, service_limit.last_reset))
                    .unwrap_or((0, now));
                require!(
                    service_input.daily_limit >= daily_spent,
                    VelaError::InvalidAmount
                );

                Ok(ServiceLimit {
                    service: service_input.service,
                    daily_limit: service_input.daily_limit,
                    daily_spent,
                    last_reset,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        mandate.services = updated_services;
    }

    write_agent_mandate(&mandate_info, &mandate, legacy_layout)?;

    emit!(AgentMandateAdjusted {
        mandate: ctx.accounts.agent_mandate.key(),
        authority: mandate.authority,
        agent: mandate.agent,
        daily_limit: mandate.daily_limit,
        lifetime_cap: mandate.lifetime_cap,
        min_pull_amount: mandate.min_pull_amount,
        min_pull_interval: mandate.min_pull_interval,
        daily_spent: mandate.daily_spent,
        total_spent: mandate.total_spent,
        remaining_balance: ctx.accounts.mandate_wrapped_account.amount,
    });

    Ok(())
}

#[event]
pub struct AgentMandateAdjusted {
    pub mandate: Pubkey,
    pub authority: Pubkey,
    pub agent: Pubkey,
    pub daily_limit: u64,
    pub lifetime_cap: u64,
    pub min_pull_amount: u64,
    pub min_pull_interval: i64,
    pub daily_spent: u64,
    pub total_spent: u64,
    pub remaining_balance: u64,
}
