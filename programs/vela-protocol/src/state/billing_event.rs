//! Billing-event account storage plus the frozen stream event contract.
//!
//! Stream `schema_version` is intentionally the first field in every stream event
//! and all future evolution is additive-only so downstream consumers can safely
//! decode `schema_version: 1` without field reordering.

use anchor_lang::prelude::*;

#[account]
pub struct BillingEvent {
    pub mandate: Pubkey,                // 32
    pub merchant: Pubkey,               // 32
    pub subscriber: Pubkey,             // 32
    pub plan_id: u64,                   // 8
    pub encrypted_blob: [[u8; 32]; 10], // 320 bytes of encrypted data (extended for usage fields)
    // Contains (encrypted): amount, timestamp, pulls_executed,
    // billing_period, payment_method (delegate vs hook),
    // billing_type, usage_units, tier_applied (usage-based billing fields)
    pub nonce: u128,     // 16
    pub created_at: i64, // 8
    pub bump: u8,        // 1
}

impl BillingEvent {
    pub const SEED_PREFIX: &'static [u8] = b"billing";
    // Seeds: [b"billing", mandate.key().as_ref(), pulls_executed.to_le_bytes().as_ref()]
    // Unique per pull execution, immutable (D-10: no close authority)
    pub const SIZE: usize = 8 + 32 + 32 + 32 + 8 + (32 * 10) + 16 + 8 + 1; // 457 bytes
}

#[event]
pub struct StreamCreated {
    pub schema_version: u8,
    pub mandate: Pubkey,
    pub subscriber: Pubkey,
    pub merchant: Pubkey,
    pub mint: Pubkey,
    pub rate_per_second: u64,
    pub authorized_max_rate: u64,
    pub max_streamed: Option<u64>,
    pub min_settle_interval: u32,
    pub timestamp: i64,
}

#[event]
pub struct StreamSettled {
    pub schema_version: u8,
    pub mandate: Pubkey,
    pub amount: u64,
    pub total_streamed_after: u64,
    pub last_settled_ts: i64,
    pub timestamp: i64,
}

#[event]
pub struct StreamPaused {
    pub schema_version: u8,
    pub mandate: Pubkey,
    pub paused_at: i64,
    pub signer: Pubkey,
    pub final_settle_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct StreamResumed {
    pub schema_version: u8,
    pub mandate: Pubkey,
    pub resumed_at: i64,
    pub pause_duration_secs: u64,
    pub signer: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct StreamRateUpdated {
    pub schema_version: u8,
    pub mandate: Pubkey,
    pub old_rate_per_second: u64,
    pub new_rate_per_second: u64,
    pub old_authorized_max_rate: u64,
    pub new_authorized_max_rate: u64,
    pub signer: Pubkey,
    pub final_settle_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct StreamCancelled {
    pub schema_version: u8,
    pub mandate: Pubkey,
    pub cancelled_at: i64,
    pub signer: Pubkey,
    pub final_settle_amount: u64,
    pub total_streamed_final: u64,
    pub timestamp: i64,
}
