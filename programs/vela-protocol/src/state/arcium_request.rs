use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArciumRequestFlow {
    Validation,
    UsageComputation,
    BillingRecord,
}

impl ArciumRequestFlow {
    pub const VALIDATION_SEED: &'static [u8] = b"validation";
    pub const USAGE_COMPUTATION_SEED: &'static [u8] = b"usage";
    pub const BILLING_RECORD_SEED: &'static [u8] = b"billing";
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArciumRequestStatus {
    Pending,
    Completed,
    Aborted,
}

#[account]
pub struct ArciumRequestState {
    pub mandate: Pubkey,
    pub flow: ArciumRequestFlow,
    pub subject: [u8; 8],
    pub computation_offset: u64,
    pub request_nonce: u64,
    pub status: ArciumRequestStatus,
    pub created_at: i64,
    pub completed_at: i64,
    pub bump: u8,
    pub version: u8,
    pub _reserved: [u8; 32],
}

impl ArciumRequestState {
    pub const SEED_PREFIX: &'static [u8] = b"arcium_request";
    pub const SIZE: usize = 8 + 32 + 1 + 8 + 8 + 8 + 1 + 8 + 8 + 1 + 1 + 32;
}

#[cfg(test)]
mod tests {
    use super::ArciumRequestState;

    #[test]
    fn size_matches_disc_plus_fields() {
        assert_eq!(ArciumRequestState::SIZE, 116);
    }
}
