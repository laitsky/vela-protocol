use anchor_lang::prelude::*;

#[error_code]
pub enum VelaError {
    #[msg("Pull attempted before next_payment_due")]
    PullTooEarly = 0,

    #[msg("Mandate has expired or been cancelled")]
    MandateNotActive = 1,

    #[msg("Maximum pulls exhausted for this mandate")]
    MaxPullsExceeded = 2,

    #[msg("Subscriber has insufficient token balance")]
    InsufficientBalance = 3,

    #[msg("Only the subscriber can cancel their mandate")]
    UnauthorizedCancel = 4,

    #[msg("Plan frequency below minimum (3600 seconds)")]
    FrequencyTooLow = 5,

    #[msg("Arithmetic overflow")]
    Overflow = 6,

    #[msg("Plan is not active")]
    PlanNotActive = 7,

    #[msg("Mandate has expired")]
    MandateExpired = 8,

    #[msg("Pull amount exceeds plan amount")]
    AmountExceedsPlanAmount = 9,

    #[msg("Plan max_pulls must be at least 1")]
    MaxPullsTooLow = 10,

    #[msg("No valid PullApproval PDA exists -- Arcium validation required")]
    ApprovalNotGranted = 11,

    #[msg("PullApproval has expired (past valid_until timestamp)")]
    ApprovalExpired = 12,

    #[msg("Arcium computation was aborted or produced invalid output")]
    AbortedComputation = 13,

    #[msg("Arcium cluster not configured in ProtocolConfig")]
    ClusterNotSet = 14,

    #[msg("Only the protocol admin can perform this action")]
    UnauthorizedAdmin = 15,

    #[msg("PullApproval already exists for this billing period")]
    ApprovalAlreadyExists = 16,

    #[msg("Arcium is unavailable -- pull blocked (fail-closed per D-01)")]
    ArciumUnavailable = 17,

    #[msg("Encrypted input payload length does not match the circuit interface")]
    InvalidCiphertextInput = 18,

    #[msg("The previous pull is missing its billing record; finalize billing before executing again")]
    PendingBillingRecord = 19,

    #[msg("BillingEvent already exists for this pull execution")]
    BillingEventAlreadyExists = 20,

    #[msg("Arcium account set does not match the configured canonical PDAs")]
    InvalidArciumAccount = 21,

    #[msg("ProtocolConfig contains an invalid or inconsistent Arcium cluster configuration")]
    InvalidProtocolConfig = 22,

    #[msg("The supplied Arcium callback does not match the queued Vela request")]
    InvalidCallbackBinding = 23,

    #[msg("Computation offset must match the protocol-derived request identity")]
    InvalidComputationOffset = 24,

    #[msg("Wrapped USDC mint has already been initialized")]
    WrappedMintAlreadyInitialized = 25,

    #[msg("Transfer not authorized -- no valid PullApproval and not a wrap/unwrap operation")]
    TransferNotAuthorized = 26,

    #[msg("Wrapping vault mismatch -- vault address does not match ProtocolConfig")]
    VaultMismatch = 27,

    #[msg("SPL USDC mint mismatch -- provided mint is not the expected USDC mint")]
    UsdcMintMismatch = 28,

    #[msg("Wrapped USDC mint not yet initialized in ProtocolConfig")]
    WrappedMintNotInitialized = 29,

    #[msg("Only the authorized agent signer can execute this pull")]
    UnauthorizedAgent = 30,

    #[msg("Destination service is not authorized on this agent mandate")]
    UnauthorizedService = 31,

    #[msg("Agent mandate service list exceeds the maximum allowed entries")]
    TooManyServices = 32,

    #[msg("Agent mandate service list contains a duplicate service")]
    DuplicateService = 33,

    #[msg("Agent mandate is paused and cannot process pulls")]
    MandatePaused = 34,

    #[msg("Agent mandate is revoked and cannot process pulls")]
    MandateRevoked = 35,

    #[msg("Agent mandate daily limit would be exceeded by this pull")]
    DailyLimitExceeded = 36,

    #[msg("Service daily limit would be exceeded by this pull")]
    ServiceDailyLimitExceeded = 37,

    #[msg("Agent mandate lifetime cap would be exceeded by this pull")]
    LifetimeCapExceeded = 38,

    #[msg("Pull amount is below the agent mandate minimum pull amount")]
    PullAmountTooSmall = 39,

    #[msg("Agent mandate pull cooldown is still active")]
    PullCooldownActive = 40,

    #[msg("Agent mandate service list is invalid")]
    InvalidServiceList = 41,

    #[msg("Agent mandate already exists for this authority and agent")]
    AgentMandateAlreadyExists = 42,

    #[msg("Agent mandate account was not found")]
    AgentMandateNotFound = 43,

    #[msg("Mandate-owned wrapped token account has insufficient balance")]
    InsufficientMandateBalance = 44,

    #[msg("Agent mandate has no funds available to drain")]
    NoFundsToDrain = 45,

    #[msg("Requested agent mandate status transition is invalid")]
    InvalidAgentMandateStatusTransition = 46,

    #[msg("Only the agent mandate authority can perform this action")]
    UnauthorizedAgentMandateAuthority = 47,

    #[msg("Keeper endpoint exceeds maximum length (128 bytes)")]
    EndpointTooLong = 48,

    #[msg("Keeper endpoint must not be empty")]
    EndpointEmpty = 49,

    #[msg("Keeper authority must not be the default (zero) public key")]
    InvalidKeeperAuthority = 50,

    #[msg("Caller is not the authorized keeper")]
    UnauthorizedKeeper = 51,

    #[msg("At least one field must be provided for update")]
    NoUpdateProvided = 52,

    #[msg("Usage plan requires 1-5 pricing tiers")]
    InvalidTierCount = 53,

    #[msg("Operation requires a different billing type")]
    BillingTypeMismatch = 54,

    #[msg("Invalid period boundaries")]
    InvalidPeriod = 55,

    #[msg("Usage report period must align with mandate next_payment_due")]
    PeriodMismatch = 56,

    #[msg("Amount must be greater than zero")]
    InvalidAmount = 57,

    #[msg("Settlement frequency must be greater than zero")]
    InvalidFrequency = 58,

    #[msg("Tier boundaries must be monotonically increasing")]
    InvalidTierBoundary = 59,

    #[msg("Protocol is paused -- billing pulls are blocked")]
    ProtocolPaused = 60,

    #[msg("Usage report has already been settled")]
    UsageReportAlreadySettled = 61,

    #[msg("Mandate version is unsupported by this instruction")]
    MandateVersionUnsupported = 100,

    #[msg("Plan version is unsupported by this instruction")]
    PlanVersionUnsupported = 200,

    #[msg("Migration preconditions are not satisfied")]
    MigrationPreconditionFailed = 300,

    #[msg("Agent mandate version is unsupported by this instruction")]
    AgentMandateVersionUnsupported = 400,
}
