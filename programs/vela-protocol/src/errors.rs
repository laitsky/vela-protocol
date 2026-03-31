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
}
