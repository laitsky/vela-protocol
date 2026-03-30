use anchor_lang::prelude::*;

#[account]
pub struct VelaPlan {
    pub merchant: Pubkey,
    pub plan_id: u64,
    pub amount: u64,
    pub frequency: u64,
    pub trial_period: u64,
    pub max_pulls: u64,
    pub status: PlanStatus,
    pub credential_mint: Pubkey,
    pub bump: u8,
}

impl VelaPlan {
    pub const SEED_PREFIX: &'static [u8] = b"plan";
    pub const SIZE: usize = 8 + 32 + 8 + 8 + 8 + 8 + 8 + 1 + 32 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum PlanStatus {
    Active,
    Inactive,
}
