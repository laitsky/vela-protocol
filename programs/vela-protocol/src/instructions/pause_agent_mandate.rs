use anchor_lang::prelude::*;

use crate::{
    constants::AGENT_MANDATE_SEED,
    errors::VelaError,
    instructions::agent_mandate_account::{load_agent_mandate, write_agent_mandate},
    state::AgentMandateStatus,
};

#[derive(Accounts)]
pub struct PauseAgentMandate<'info> {
    pub authority: Signer<'info>,

    /// CHECK: Used for PDA derivation only.
    pub agent: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [AGENT_MANDATE_SEED, authority.key().as_ref(), agent.key().as_ref()],
        bump,
    )]
    pub agent_mandate: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<PauseAgentMandate>) -> Result<()> {
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
        AgentMandateStatus::Active => {
            mandate.status = AgentMandateStatus::Paused;
        }
        AgentMandateStatus::Paused | AgentMandateStatus::Revoked => {
            return Err(VelaError::InvalidAgentMandateStatusTransition.into());
        }
    }

    write_agent_mandate(&mandate_info, &mandate, legacy_layout)?;

    emit!(AgentMandatePaused {
        mandate: ctx.accounts.agent_mandate.key(),
        authority: mandate.authority,
        agent: mandate.agent,
        daily_spent: mandate.daily_spent,
        total_spent: mandate.total_spent,
    });

    Ok(())
}

#[event]
pub struct AgentMandatePaused {
    pub mandate: Pubkey,
    pub authority: Pubkey,
    pub agent: Pubkey,
    pub daily_spent: u64,
    pub total_spent: u64,
}
