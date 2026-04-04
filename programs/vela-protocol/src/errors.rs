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

    #[msg("Wrapped USDC mint has already been initialized")]
    WrappedMintAlreadyInitialized,

    #[msg("Transfer not authorized -- no valid PullApproval and not a wrap/unwrap operation")]
    TransferNotAuthorized,

    #[msg("Wrapping vault mismatch -- vault address does not match ProtocolConfig")]
    VaultMismatch,

    #[msg("SPL USDC mint mismatch -- provided mint is not the expected USDC mint")]
    UsdcMintMismatch,

    #[msg("Wrapped USDC mint not yet initialized in ProtocolConfig")]
    WrappedMintNotInitialized,

    #[msg("Keeper endpoint exceeds maximum length (128 bytes)")]
    EndpointTooLong,

    #[msg("Keeper endpoint must not be empty")]
    EndpointEmpty,

    #[msg("Keeper authority must not be the default (zero) public key")]
    InvalidKeeperAuthority,

    #[msg("Caller is not the authorized keeper")]
    UnauthorizedKeeper,

    #[msg("At least one field must be provided for update")]
    NoUpdateProvided,

    #[msg("Usage plan requires 1-5 pricing tiers")]
    InvalidTierCount,

    #[msg("Operation requires a different billing type")]
    BillingTypeMismatch,

    #[msg("Invalid period boundaries")]
    InvalidPeriod,

    #[msg("Usage report period must align with mandate next_payment_due")]
    PeriodMismatch,

    #[msg("Amount must be greater than zero")]
    InvalidAmount,

    #[msg("Settlement frequency must be greater than zero")]
    InvalidFrequency,

    #[msg("Tier boundaries must be monotonically increasing")]
    InvalidTierBoundary,

    #[msg("Protocol is paused -- billing pulls are blocked")]
    ProtocolPaused,

    #[msg("Usage report has already been settled")]
    UsageReportAlreadySettled,
}
