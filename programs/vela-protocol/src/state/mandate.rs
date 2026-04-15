use anchor_lang::prelude::*;

use super::billing_type::BillingType;
use super::CURRENT_ACCOUNT_VERSION;

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
    pub mandate_index: u64,
    pub version: u8,
    pub credit_balance: u64,
    pub pending_new_plan: Pubkey,
    pub pending_effective_at: i64,
    pub pending_change_type: u8,
    pub pending_nonce_short: [u8; 8],
    pub _reserved_v3: [u8; 7],
}

impl VelaMandate {
    pub const SEED_PREFIX: &'static [u8] = b"mandate";
    pub const SIZE: usize = 8 + 32 + 32 + 32 + (11 * 8) + 1 + 1 + 1 + 8 + 1 + 8 + 32 + 8 + 1 + 8 + 7;

    pub fn current_version() -> u8 {
        CURRENT_ACCOUNT_VERSION
    }

    pub fn has_pending_change(&self) -> bool {
        self.pending_change_type != 0
    }

    pub fn clear_pending(&mut self) {
        self.pending_new_plan = Pubkey::default();
        self.pending_effective_at = 0;
        self.pending_change_type = 0;
        self.pending_nonce_short = [0; 8];
        self._reserved_v3 = [0; 7];
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum MandateStatus {
    Active,
    Cancelled,
    Expired,
}

#[cfg(test)]
mod tests {
    use super::VelaMandate;

    #[test]
    fn size_matches_disc_plus_fields() {
        assert_eq!(VelaMandate::SIZE, 268);
    }
}
