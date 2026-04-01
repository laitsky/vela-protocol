use anchor_lang::prelude::*;

use super::billing_type::BillingType;

#[account]
pub struct VelaMandate {
    pub subscriber: Pubkey,
    pub plan: Pubkey,
    pub merchant: Pubkey,
    pub amount: u64,
    pub frequency: u64,
    pub start_date: i64,
    pub expiry: i64,
    pub max_pulls: u64,
    pub pulls_executed: u64,
    pub next_payment_due: i64,
    pub last_pull_at: i64,
    pub last_billing_recorded_pull: u64,
    pub validation_request_nonce: u64,
    pub billing_request_nonce: u64,
    pub status: MandateStatus,
    pub bump: u8,
    pub billing_type: BillingType, // 1 - trailing field for backward compat (0u8 = Flat)
}

impl VelaMandate {
    pub const SEED_PREFIX: &'static [u8] = b"mandate";
    // Previous SIZE: 8 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1 + 1 = 170
    // +1 for billing_type field
    pub const SIZE: usize = 8 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8 + 1 + 1 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum MandateStatus {
    Active,
    Cancelled,
    Expired,
}
