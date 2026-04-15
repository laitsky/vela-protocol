use anchor_lang::prelude::*;

use super::{ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION};

#[account]
pub struct MerchantState {
    pub merchant: Pubkey,
    pub plan_count: u64,
    pub bump: u8,
    pub credential_mint: Pubkey,
    pub mandate_counter: u64,
    pub stream_mandate_counter: u64,
    pub version: u8,
    pub _reserved: [u8; ACCOUNT_RESERVED_BYTES - 8],
}

impl MerchantState {
    pub const SEED_PREFIX: &'static [u8] = b"merchant";
    pub const SIZE: usize = 8 + 32 + 8 + 1 + 32 + 8 + 8 + 1 + (ACCOUNT_RESERVED_BYTES - 8);

    pub fn new(merchant: Pubkey, bump: u8, plan_count: u64) -> Self {
        Self {
            merchant,
            plan_count,
            bump,
            credential_mint: Pubkey::default(),
            mandate_counter: 0,
            stream_mandate_counter: 0,
            version: CURRENT_ACCOUNT_VERSION,
            _reserved: [0; ACCOUNT_RESERVED_BYTES - 8],
        }
    }
}
