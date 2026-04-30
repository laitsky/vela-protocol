#[path = "helpers/mod.rs"]
mod helpers;

use helpers::TestHarness;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{PlanStatus, VelaPlan, CURRENT_ACCOUNT_VERSION},
};

/// Test: update_plan instruction allows merchant to modify flat plan fields.
/// Verifies the plan state is updated and old/new values are accessible.
#[test]
fn test_update_flat_plan_modifies_fields() {
    let mut harness = TestHarness::new();

    let initial_amount = 25_000_000u64;
    let initial_frequency = MIN_FREQUENCY_SECONDS;
    let initial_trial = 86_400u64;
    let initial_max_pulls = 12u64;

    let addresses = harness.derive_plan_addresses(0);
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(
            initial_amount,
            initial_frequency,
            initial_trial,
            initial_max_pulls,
            0,
        )
        .expect("create_plan should succeed");

    // Verify initial state
    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    assert_eq!(plan.amount, initial_amount);
    assert_eq!(plan.frequency, initial_frequency);
    assert_eq!(plan.trial_period, initial_trial);
    assert_eq!(plan.max_pulls, initial_max_pulls);

    // Update the plan with new values
    let new_amount = 30_000_000u64;
    let new_frequency = MIN_FREQUENCY_SECONDS * 2;
    let new_trial = 172_800u64;
    let new_max_pulls = 24u64;

    harness
        .send_update_plan(
            0,
            Some(new_amount),
            Some(new_frequency),
            Some(new_trial),
            Some(new_max_pulls),
        )
        .expect("update_plan should succeed");

    let updated: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    assert_eq!(updated.amount, new_amount);
    assert_eq!(updated.frequency, new_frequency);
    assert_eq!(updated.trial_period, new_trial);
    assert_eq!(updated.max_pulls, new_max_pulls);

    // Versioning preserved through update
    assert_eq!(updated.version, CURRENT_ACCOUNT_VERSION);
    assert_eq!(
        updated._reserved,
        [0u8; vela_protocol::state::PLAN_RESERVED_BYTES]
    );

    // Immutable fields should NOT change
    assert_eq!(updated.merchant, harness.merchant_pubkey());
    assert_eq!(updated.plan_id, 0);
    assert!(matches!(updated.status, PlanStatus::Active));
    assert_eq!(updated.credential_mint, plan.credential_mint);
    assert_eq!(updated.bump, plan.bump);
}

/// Test: update_plan rejects non-merchant signers.
#[test]
fn test_update_flat_plan_rejects_non_merchant() {
    let mut harness = TestHarness::new();

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");

    let imposter = harness.create_wallet();

    let result = harness.send_update_plan_as(&imposter, 0, Some(50_000_000), None, None, None);

    assert!(
        result.is_err(),
        "update_plan must reject non-merchant signer"
    );
}

/// Test: update_plan with all None fields returns NoUpdateProvided error.
#[test]
fn test_update_flat_plan_rejects_empty_update() {
    let mut harness = TestHarness::new();

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");

    let result = harness.send_update_plan(0, None, None, None, None);

    assert!(result.is_err(), "update_plan must reject empty update");
}

/// Test: update_plan can update individual fields without affecting others.
#[test]
fn test_update_flat_plan_partial_update() {
    let mut harness = TestHarness::new();

    let initial_amount = 25_000_000u64;
    let initial_frequency = MIN_FREQUENCY_SECONDS;
    let initial_trial = 86_400u64;
    let initial_max_pulls = 12u64;

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(
            initial_amount,
            initial_frequency,
            initial_trial,
            initial_max_pulls,
            0,
        )
        .expect("create_plan should succeed");

    // Only update amount
    let new_amount = 99_000_000u64;
    harness
        .send_update_plan(0, Some(new_amount), None, None, None)
        .expect("update_plan with only amount should succeed");

    let addresses = harness.derive_plan_addresses(0);
    let updated: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    assert_eq!(updated.amount, new_amount, "amount should be updated");
    assert_eq!(
        updated.frequency, initial_frequency,
        "frequency should be unchanged"
    );
    assert_eq!(
        updated.trial_period, initial_trial,
        "trial_period should be unchanged"
    );
    assert_eq!(
        updated.max_pulls, initial_max_pulls,
        "max_pulls should be unchanged"
    );
}
