use anchor_lang::prelude::*;

use super::CURRENT_ACCOUNT_VERSION;

#[account]
pub struct StreamMandate {
    pub version: u8,
    pub subscriber: Pubkey,
    pub merchant: Pubkey,
    pub mint: Pubkey,
    pub rate_per_second: u64,
    pub authorized_max_rate: u64,
    pub last_settled_ts: i64,
    pub total_streamed: u64,
    pub max_streamed: Option<u64>,
    pub paused_at: Option<i64>,
    pub min_settle_interval: u32,
    pub status: StreamStatus,
    pub mandate_index: u64,
    pub bump: u8,
    pub pending_new_rate_per_second: u64,
    pub pending_new_authorized_max_rate: u64,
    pub pending_effective_at: i64,
    pub pending_change_type: u8,
    pub pending_nonce_short: [u8; 8],
    pub _reserved_v2: [u8; 23],
}

impl StreamMandate {
    pub const SEED_PREFIX: &'static [u8] = b"stream";
    pub const SIZE: usize = 8 + 1 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 9 + 9 + 4 + 1 + 8 + 1 + 56;

    pub fn current_version() -> u8 {
        CURRENT_ACCOUNT_VERSION
    }

    pub fn has_pending_rate_change(&self) -> bool {
        self.pending_change_type != 0
    }

    pub fn clear_pending_rate_change(&mut self) {
        self.pending_new_rate_per_second = 0;
        self.pending_new_authorized_max_rate = 0;
        self.pending_effective_at = 0;
        self.pending_change_type = 0;
        self.pending_nonce_short = [0; 8];
        self._reserved_v2 = [0; 23];
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum StreamStatus {
    Active,
    Paused,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::StreamMandate;

    #[test]
    fn size_matches_disc_plus_fields() {
        assert_eq!(StreamMandate::SIZE, 225);
    }
}
