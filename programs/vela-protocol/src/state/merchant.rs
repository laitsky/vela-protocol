use anchor_lang::prelude::*;

#[account]
pub struct MerchantState {
    pub merchant: Pubkey,
    pub plan_count: u64,
    pub bump: u8,
}

impl MerchantState {
    pub const SEED_PREFIX: &'static [u8] = b"merchant";
    pub const SIZE: usize = 8 + 32 + 8 + 1;
}
