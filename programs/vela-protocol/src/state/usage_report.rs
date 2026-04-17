use anchor_lang::prelude::*;

/// UsageReport PDA — encrypted usage data submitted by a merchant for a billing period.
///
/// Seeds: [b"usage_report", mandate.key().as_ref(), period_start.to_le_bytes().as_ref()]
/// SIZE: 8 + 32 + 32 + 8 + 8 + (32 * 4) + 16 + 32 + 1 + 8 + 1 = 274 bytes
#[account]
pub struct UsageReport {
    pub mandate: Pubkey,                // 32
    pub merchant: Pubkey,               // 32
    pub period_start: i64,              // 8
    pub period_end: i64,                // 8
    pub encrypted_usage: [[u8; 32]; 4], // 128 - encrypted usage data (Arcium ciphertext)
    pub nonce: u128,                    // 16 - encryption nonce
    pub pub_key: [u8; 32],              // 32 - client x25519 public key
    pub settled: bool,                  // 1 - true once billing pull executed
    pub created_at: i64,                // 8
    pub bump: u8,                       // 1
}

impl UsageReport {
    pub const SEED_PREFIX: &'static [u8] = b"usage_report";
    pub const SIZE: usize = 8 + 32 + 32 + 8 + 8 + (32 * 4) + 16 + 32 + 1 + 8 + 1; // 274 bytes
}
