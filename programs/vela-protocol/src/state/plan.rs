use anchor_lang::prelude::*;

pub const PLAN_RESERVED_BYTES: usize = 32;

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
    pub billing_mint: Pubkey,
    pub bump: u8,
    pub version: u8,
    pub _reserved: [u8; PLAN_RESERVED_BYTES],
}

impl VelaPlan {
    pub const SEED_PREFIX: &'static [u8] = b"plan";
    pub const SIZE: usize = 8 + 32 + 8 + 8 + 8 + 8 + 8 + 1 + 32 + 32 + 1 + 1 + PLAN_RESERVED_BYTES;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum PlanStatus {
    Active,
    Inactive,
}
