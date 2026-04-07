use anchor_lang::prelude::*;

use crate::constants::{
    AGENT_DAILY_RESET_WINDOW_SECONDS, AGENT_MANDATE_MAX_SERVICES, AGENT_MANDATE_SEED,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub struct ServiceLimit {
    pub service: Pubkey,
    pub daily_limit: u64,
    pub daily_spent: u64,
    pub last_reset: i64,
}

impl ServiceLimit {
    pub const SIZE: usize = 32 + 8 + 8 + 8;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum AgentMandateStatus {
    Active,
    Paused,
    Revoked,
}

#[account]
pub struct AgentMandate {
    pub authority: Pubkey,
    pub agent: Pubkey,
    pub daily_limit: u64,
    pub daily_spent: u64,
    pub daily_last_reset: i64,
    pub lifetime_cap: u64,
    pub total_spent: u64,
    pub min_pull_amount: u64,
    pub min_pull_interval: i64,
    pub last_pull_at: i64,
    pub status: AgentMandateStatus,
    pub services: Vec<ServiceLimit>,
    pub bump: u8,
}

impl AgentMandate {
    pub const SEED_PREFIX: &'static [u8] = AGENT_MANDATE_SEED;
    pub const MAX_SERVICES: usize = AGENT_MANDATE_MAX_SERVICES;
    pub const BASE_SIZE: usize = 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1 + 1;
    pub const SIZE: usize = 8 + Self::BASE_SIZE + 4 + (Self::MAX_SERVICES * ServiceLimit::SIZE);

    pub fn reset_daily_if_needed(&mut self, now: i64) {
        if now.saturating_sub(self.daily_last_reset) >= AGENT_DAILY_RESET_WINDOW_SECONDS {
            self.daily_spent = 0;
            self.daily_last_reset = now;
        }
    }

    pub fn find_service_index(&self, service: &Pubkey) -> Option<usize> {
        self.services
            .iter()
            .position(|service_limit| service_limit.service == *service)
    }

    pub fn reset_service_daily_if_needed(&mut self, index: usize, now: i64) {
        if let Some(service_limit) = self.services.get_mut(index) {
            if now.saturating_sub(service_limit.last_reset) >= AGENT_DAILY_RESET_WINDOW_SECONDS {
                service_limit.daily_spent = 0;
                service_limit.last_reset = now;
            }
        }
    }
}
