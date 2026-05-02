#[path = "helpers/mod.rs"]
mod helpers;

use helpers::TestHarness;
use vela_protocol::{
    constants::{MAX_SAFE_USAGE_CHARGE, MAX_SAFE_USAGE_RATE_PER_UNIT},
    state::{PlanStatus, PricingTier, UsagePlan, ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION},
};

/// Test: update_usage_plan instruction allows merchant to modify usage plan fields.
#[test]
fn test_update_usage_plan_modifies_fields() {
    let mut harness = TestHarness::new();

    let plan_id = 10u64;
    let unit_name = *b"api_calls\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let initial_tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let initial_max_charge = 50_000_000u64;
    let initial_settlement = 3600u64;

    let addresses = harness.derive_usage_plan_addresses(plan_id);
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_usage_plan(
            plan_id,
            unit_name,
            initial_tiers.clone(),
            initial_max_charge,
            initial_settlement,
        )
        .expect("create_usage_plan should succeed");

    // Verify initial state
    let plan: UsagePlan = harness.fetch_anchor_account(&addresses.usage_plan);
    assert_eq!(plan.max_charge_per_period, initial_max_charge);
    assert_eq!(plan.settlement_frequency, initial_settlement);

    // Update the usage plan
    let new_tiers = vec![
        PricingTier {
            up_to: 1000,
            rate_per_unit: 50,
            _padding: 0,
        },
        PricingTier {
            up_to: 0,
            rate_per_unit: 25,
            _padding: 0,
        },
    ];
    let new_max_charge = 100_000_000u64;
    let new_settlement = 7200u64;

    harness
        .send_update_usage_plan(
            plan_id,
            Some(new_tiers.clone()),
            Some(new_max_charge),
            Some(new_settlement),
        )
        .expect("update_usage_plan should succeed");

    let updated: UsagePlan = harness.fetch_anchor_account(&addresses.usage_plan);
    assert_eq!(updated.max_charge_per_period, new_max_charge);
    assert_eq!(updated.settlement_frequency, new_settlement);
    assert_eq!(updated.tier_count, new_tiers.len() as u8);
    assert_eq!(updated.tiers[0], new_tiers[0]);
    assert_eq!(updated.tiers[1], new_tiers[1]);

    // Versioning preserved
    assert_eq!(updated.version, CURRENT_ACCOUNT_VERSION);
    assert_eq!(updated._reserved, [0u8; ACCOUNT_RESERVED_BYTES]);

    // Immutable fields should NOT change
    assert_eq!(updated.merchant, harness.merchant_pubkey());
    assert_eq!(updated.plan_id, plan_id);
    assert_eq!(updated.unit_name, unit_name);
    assert!(matches!(updated.status, PlanStatus::Active));
    assert_eq!(updated.credential_mint, plan.credential_mint);
    assert_eq!(updated.bump, plan.bump);
}

/// Test: update_usage_plan rejects non-merchant signers.
#[test]
fn test_update_usage_plan_rejects_non_merchant() {
    let mut harness = TestHarness::new();

    let plan_id = 20u64;
    let unit_name = [0u8; 32];
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_usage_plan(plan_id, unit_name, tiers, 50_000_000, 3600)
        .expect("create_usage_plan should succeed");

    let imposter = harness.create_wallet();

    let result =
        harness.send_update_usage_plan_as(&imposter, plan_id, None, Some(100_000_000), None);

    assert!(
        result.is_err(),
        "update_usage_plan must reject non-merchant signer"
    );
}

/// Test: update_usage_plan with all None fields returns NoUpdateProvided error.
#[test]
fn test_update_usage_plan_rejects_empty_update() {
    let mut harness = TestHarness::new();

    let plan_id = 30u64;
    let unit_name = [0u8; 32];
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_usage_plan(plan_id, unit_name, tiers, 50_000_000, 3600)
        .expect("create_usage_plan should succeed");

    let result = harness.send_update_usage_plan(plan_id, None, None, None);

    assert!(
        result.is_err(),
        "update_usage_plan must reject empty update"
    );
}

/// Test: update_usage_plan partial update only changes specified fields.
#[test]
fn test_update_usage_plan_partial_update() {
    let mut harness = TestHarness::new();

    let plan_id = 40u64;
    let unit_name = *b"bytes\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let initial_tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let initial_max_charge = 50_000_000u64;
    let initial_settlement = 3600u64;

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_usage_plan(
            plan_id,
            unit_name,
            initial_tiers,
            initial_max_charge,
            initial_settlement,
        )
        .expect("create_usage_plan should succeed");

    // Only update max_charge_per_period
    let new_max_charge = 200_000_000u64;
    let addresses = harness.derive_usage_plan_addresses(plan_id);
    harness
        .send_update_usage_plan(plan_id, None, Some(new_max_charge), None)
        .expect("partial update_usage_plan should succeed");

    let updated: UsagePlan = harness.fetch_anchor_account(&addresses.usage_plan);
    assert_eq!(updated.max_charge_per_period, new_max_charge);
    assert_eq!(
        updated.settlement_frequency, initial_settlement,
        "settlement_frequency should be unchanged"
    );
    assert_eq!(updated.tier_count, 1, "tier_count should be unchanged");
}

#[test]
fn test_update_usage_plan_rejects_overflow_risk_rate() {
    let mut harness = TestHarness::new();

    let plan_id = 50u64;
    let unit_name = [0u8; 32];
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_usage_plan(plan_id, unit_name, tiers, 50_000_000, 3600)
        .expect("create_usage_plan should succeed");

    let unsafe_tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: MAX_SAFE_USAGE_RATE_PER_UNIT + 1,
        _padding: 0,
    }];

    let error = harness
        .send_update_usage_plan(plan_id, Some(unsafe_tiers), None, None)
        .expect_err("unsafe usage rate must be rejected");

    assert!(
        format!("{:?}", error.err).contains("Custom(6064)"),
        "expected UsagePricingOverflowRisk custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_update_usage_plan_rejects_overflow_risk_cap_only_update() {
    let mut harness = TestHarness::new();

    let plan_id = 51u64;
    let unit_name = [0u8; 32];
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_usage_plan(plan_id, unit_name, tiers, 50_000_000, 3600)
        .expect("create_usage_plan should succeed");

    let error = harness
        .send_update_usage_plan(plan_id, None, Some(MAX_SAFE_USAGE_CHARGE + 1), None)
        .expect_err("unsafe max charge must be rejected");

    assert!(
        format!("{:?}", error.err).contains("Custom(6064)"),
        "expected UsagePricingOverflowRisk custom error, got {:?}",
        error.err,
    );
}
