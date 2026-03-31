use anchor_lang::prelude::*;

#[error_code]
pub enum VelaError {
    #[msg("Pull attempted before next_payment_due")]
    PullTooEarly,

    #[msg("Mandate has expired or been cancelled")]
    MandateNotActive,

    #[msg("Maximum pulls exhausted for this mandate")]
    MaxPullsExceeded,

    #[msg("Subscriber has insufficient token balance")]
    InsufficientBalance,

    #[msg("Only the subscriber can cancel their mandate")]
    UnauthorizedCancel,

    #[msg("Plan frequency below minimum (3600 seconds)")]
    FrequencyTooLow,

    #[msg("Arithmetic overflow")]
    Overflow,

    #[msg("Plan is not active")]
    PlanNotActive,

    #[msg("Mandate has expired")]
    MandateExpired,

    #[msg("Pull amount exceeds plan amount")]
    AmountExceedsPlanAmount,

    #[msg("Plan max_pulls must be at least 1")]
    MaxPullsTooLow,

    #[msg("No valid PullApproval PDA exists -- Arcium validation required")]
    ApprovalNotGranted,

    #[msg("PullApproval has expired (past valid_until timestamp)")]
    ApprovalExpired,

    #[msg("Arcium computation was aborted or produced invalid output")]
    AbortedComputation,

    #[msg("Arcium cluster not configured in ProtocolConfig")]
    ClusterNotSet,

    #[msg("Only the protocol admin can perform this action")]
    UnauthorizedAdmin,

    #[msg("PullApproval already exists for this billing period")]
    ApprovalAlreadyExists,

    #[msg("Arcium is unavailable -- pull blocked (fail-closed per D-01)")]
    ArciumUnavailable,

    #[msg("Encrypted input payload length does not match the circuit interface")]
    InvalidCiphertextInput,

    #[msg("The previous pull is missing its billing record; finalize billing before executing again")]
    PendingBillingRecord,

    #[msg("BillingEvent already exists for this pull execution")]
    BillingEventAlreadyExists,

    #[msg("Arcium account set does not match the configured canonical PDAs")]
    InvalidArciumAccount,

    #[msg("ProtocolConfig contains an invalid or inconsistent Arcium cluster configuration")]
    InvalidProtocolConfig,

    #[msg("The supplied Arcium callback does not match the queued Vela request")]
    InvalidCallbackBinding,

    #[msg("Computation offset must match the protocol-derived request identity")]
    InvalidComputationOffset,
}
