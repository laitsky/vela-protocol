use anchor_lang::prelude::*;

use super::{ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum BillingRail {
    TransferHook,  // Wrapped tokens via transfer hook CPI chain
    TokenDelegate, // Native SPL tokens via approve/transfer (v1.8)
}

#[account]
pub struct TokenConfig {
    pub mint: Pubkey,                            // 32
    pub token_program: Pubkey,                   // 32
    pub billing_rail: BillingRail,               // 1
    pub decimals: u8,                            // 1
    pub enabled: bool,                           // 1
    pub oracle_reference: Pubkey,                // 32 - Pubkey::default() until v1.8
    pub admin: Pubkey,                           // 32 - admin who registered
    pub created_at: i64,                         // 8
    pub bump: u8,                                // 1
    pub version: u8,                             // 1
    pub _reserved: [u8; ACCOUNT_RESERVED_BYTES], // 64
}

impl TokenConfig {
    pub const SEED_PREFIX: &'static [u8] = b"token_config";
    // SIZE = 8 (discriminator) + 32 + 32 + 1 + 1 + 1 + 32 + 32 + 8 + 1 + 1 + 64 = 213
    pub const SIZE: usize = 8 + 32 + 32 + 1 + 1 + 1 + 32 + 32 + 8 + 1 + 1 + ACCOUNT_RESERVED_BYTES;

    pub fn new(
        mint: Pubkey,
        token_program: Pubkey,
        billing_rail: BillingRail,
        decimals: u8,
        oracle_reference: Pubkey,
        admin: Pubkey,
        created_at: i64,
        bump: u8,
    ) -> Self {
        Self {
            mint,
            token_program,
            billing_rail,
            decimals,
            enabled: true,
            oracle_reference,
            admin,
            created_at,
            bump,
            version: CURRENT_ACCOUNT_VERSION,
            _reserved: [0; ACCOUNT_RESERVED_BYTES],
        }
    }
}
