use anchor_lang::prelude::*;

use super::plan::PlanStatus;
use super::ACCOUNT_RESERVED_BYTES;

/// A single tier in a usage-based pricing schedule.
/// 24 bytes total (3 x u64).
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Default, Debug, PartialEq)]
pub struct PricingTier {
    pub up_to: u64,         // 8 - usage units upper bound (0 = unlimited for last tier)
    pub rate_per_unit: u64, // 8 - micro-USDC per unit (6 decimals)
    pub _padding: u64,      // 8 - alignment/future use
}

/// UsagePlan PDA — a merchant's usage-based pricing plan with up to 5 tiers.
///
/// Seeds: [b"usage_plan", merchant.key().as_ref(), plan_id.to_le_bytes().as_ref()]
/// SIZE: 8 + 32 + 8 + 32 + (24 * 5) + 1 + 8 + 8 + 32 + 32 + 1 + 1 + 1 + 32 = 317 bytes
#[account]
pub struct UsagePlan {
    pub merchant: Pubkey,                             // 32
    pub plan_id: u64,                                 // 8
    pub unit_name: [u8; 32],                          // 32 - e.g., "api_calls" (UTF-8, null-padded)
    pub tiers: [PricingTier; 5],                      // 120 = 5 x 24
    pub tier_count: u8,                               // 1 - how many tiers active (1-5)
    pub max_charge_per_period: u64,                   // 8 - subscriber-approved cap per D-20
    pub settlement_frequency: u64,                    // 8 - seconds between settlements
    pub credential_mint: Pubkey,                      // 32 - same credential pattern as VelaPlan
    pub billing_mint: Pubkey,                         // 32 - Token-2022 mint charged by usage pulls
    pub status: PlanStatus,                           // 1
    pub bump: u8,                                     // 1
    pub version: u8,                                  // 1 - schema version
    pub _reserved: [u8; ACCOUNT_RESERVED_BYTES - 32], // 32 - reserved for future fields
}

impl UsagePlan {
    pub const SEED_PREFIX: &'static [u8] = b"usage_plan";
    pub const SIZE: usize = 8
        + 32
        + 8
        + 32
        + (24 * 5)
        + 1
        + 8
        + 8
        + 32
        + 32
        + 1
        + 1
        + 1
        + (ACCOUNT_RESERVED_BYTES - 32); // 317 bytes
}
