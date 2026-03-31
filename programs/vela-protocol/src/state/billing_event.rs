use anchor_lang::prelude::*;

#[account]
pub struct BillingEvent {
    pub mandate: Pubkey,               // 32
    pub merchant: Pubkey,              // 32
    pub subscriber: Pubkey,            // 32
    pub plan_id: u64,                  // 8
    pub encrypted_blob: [[u8; 32]; 8], // 256 bytes of encrypted data
    // Contains (encrypted): amount, timestamp, pulls_executed,
    // billing_period, payment_method (delegate vs hook)
    pub nonce: u128,     // 16
    pub created_at: i64, // 8
    pub bump: u8,        // 1
}

impl BillingEvent {
    pub const SEED_PREFIX: &'static [u8] = b"billing";
    // Seeds: [b"billing", mandate.key().as_ref(), pulls_executed.to_le_bytes().as_ref()]
    // Unique per pull execution, immutable (D-10: no close authority)
    pub const SIZE: usize = 8 + 32 + 32 + 32 + 8 + (32 * 8) + 16 + 8 + 1; // 393 bytes
}
