#[path = "helpers/mod.rs"]
mod helpers;

#[test]
fn test_agent_pull_within_daily_limit_and_exceed_fail() {}

#[test]
fn test_agent_pull_within_lifetime_cap_and_exceed_fail() {}

#[test]
fn test_agent_pull_per_service_limits_are_independent() {}

#[test]
fn test_agent_pull_resets_daily_spent_after_24h() {}

#[test]
fn test_agent_pull_rejects_unauthorized_service() {}

#[test]
fn test_agent_pull_enforces_min_amount_and_cooldown() {}
