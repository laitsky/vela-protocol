use anchor_lang::prelude::*;

#[account]
pub struct PullApproval {
    pub mandate: Pubkey,       // 32 - which mandate this approves
    pub valid_until: i64,      // 8 - matches mandate.next_payment_due (D-05: per-period)
    pub approved: bool,        // 1 - true if Arcium validated successfully
    pub approved_amount: u64,  // 8 - maximum transfer amount the hook should allow (D-05)
    pub created_at: i64,       // 8 - timestamp of callback
    pub bump: u8,              // 1 - PDA bump
}

impl PullApproval {
    pub const SEED_PREFIX: &'static [u8] = b"approval";
    // Seeds: [b"approval", mandate.key().as_ref()]
    // One approval per mandate at a time (D-05: per-period caching)
    pub const SIZE: usize = 8 + 32 + 8 + 1 + 8 + 8 + 1; // 66 bytes
}
