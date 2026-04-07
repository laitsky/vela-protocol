use anchor_lang::prelude::*;

use crate::{
    constants::AGENT_MANDATE_SEED,
    errors::VelaError,
    state::{AgentMandate, AgentMandateStatus},
};

#[derive(Accounts)]
pub struct PauseAgentMandate<'info> {
    pub authority: Signer<'info>,

    /// CHECK: Used for PDA derivation only.
    pub agent: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [AGENT_MANDATE_SEED, authority.key().as_ref(), agent.key().as_ref()],
        bump = agent_mandate.bump,
        constraint = agent_mandate.authority == authority.key() @ VelaError::UnauthorizedAgentMandateAuthority,
    )]
    pub agent_mandate: Account<'info, AgentMandate>,
}

pub fn handler(ctx: Context<PauseAgentMandate>) -> Result<()> {
    let mandate = &mut ctx.accounts.agent_mandate;
    match mandate.status {
        AgentMandateStatus::Active => {
            mandate.status = AgentMandateStatus::Paused;
        }
        AgentMandateStatus::Paused | AgentMandateStatus::Revoked => {
            return Err(VelaError::InvalidAgentMandateStatusTransition.into());
        }
    }

    emit!(AgentMandatePaused {
        mandate: mandate.key(),
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
