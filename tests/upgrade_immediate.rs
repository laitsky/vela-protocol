#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use solana_address::Address;
use upgrade_helpers::{
    assert_custom_error, assert_upgrade_events, fetch_mandate, setup_periodic_upgrade_fixture,
    wrapped_balance, PERIOD_SECONDS,
};
use vela_protocol::errors::VelaError;

#[test]
fn test_update_mandate_plan_charges_prorated_upgrade_and_rebinds_plan() {
    let mut fixture = setup_periodic_upgrade_fixture(10_000_000, 20_000_000);
    let half_period = (PERIOD_SECONDS / 2) as i64;
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due - half_period);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        10_000_000,
    );

    let subscriber_before = wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped);
    let merchant_before = wrapped_balance(&fixture.harness, &fixture.merchant_wrapped);
    let metadata = fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("update_mandate_plan should succeed");

    assert_upgrade_events(
        &metadata,
        fixture.mandate,
        fixture.plan_a,
        fixture.plan_b,
        5_000_000,
    );

    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.plan, fixture.plan_b);
    assert_eq!(mandate_after.amount, fixture.plan_b_state.amount);
    assert_eq!(mandate_after.frequency, fixture.plan_b_state.frequency);
    assert_eq!(mandate_after.credit_balance, 0);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before - 5_000_000
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before + 5_000_000
    );
    let approval = fixture
        .harness
        .derive_pull_approval_address(&fixture.mandate);
    let approval_account = fixture
        .harness
        .svm
        .get_account(&Address::from(approval.to_bytes()));
    assert!(
        approval_account
            .map(|account| account.lamports == 0 && account.data.is_empty())
            .unwrap_or(true),
        "positive-proration approval must be closed after successful charge"
    );
}

#[test]
fn positive_proration_closes_pull_approval() {
    let mut fixture = setup_periodic_upgrade_fixture(10_000_000, 20_000_000);
    let half_period = (PERIOD_SECONDS / 2) as i64;
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due - half_period);
    let approval = fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        10_000_000,
    );

    fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("positive proration should succeed");

    let approval_account = fixture
        .harness
        .svm
        .get_account(&Address::from(approval.to_bytes()));
    assert!(
        approval_account
            .map(|account| account.lamports == 0 && account.data.is_empty())
            .unwrap_or(true),
        "PullApproval PDA must be closed/refunded after proration transfer"
    );
}

#[test]
fn positive_proration_approval_cannot_be_reused() {
    let mut fixture = setup_periodic_upgrade_fixture(10_000_000, 20_000_000);
    let half_period = (PERIOD_SECONDS / 2) as i64;
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due - half_period);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        10_000_000,
    );

    fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("first positive proration should consume the approval");

    fixture
        .harness
        .send_create_plan(30_000_000, PERIOD_SECONDS, 0, 6, 2)
        .expect("create plan C should succeed");
    let plan_c = fixture.harness.derive_plan_addresses(2).plan;
    let subscriber_before = wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped);
    let merchant_before = wrapped_balance(&fixture.harness, &fixture.merchant_wrapped);

    let failure = fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &plan_c,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect_err("closed proration approval must not be reusable");

    assert_custom_error(&failure, VelaError::ApprovalNotGranted as u32);
    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.plan, fixture.plan_b);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before
    );
}

#[test]
fn update_mandate_plan_rejects_wrong_period_proration_approval() {
    let mut fixture = setup_periodic_upgrade_fixture(10_000_000, 20_000_000);
    let half_period = (PERIOD_SECONDS / 2) as i64;
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due - half_period);
    fixture.harness.create_pull_approval_with_period_and_amount(
        &fixture.mandate,
        mandate_before.next_payment_due - mandate_before.frequency as i64 - 1,
        mandate_before.next_payment_due,
        mandate_before.next_payment_due + 600,
        true,
        10_000_000,
    );

    let subscriber_before = wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped);
    let merchant_before = wrapped_balance(&fixture.harness, &fixture.merchant_wrapped);
    let failure = fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect_err("wrong-period proration approval should be rejected");

    assert_custom_error(&failure, VelaError::PeriodMismatch as u32);
    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.plan, fixture.plan_a);
    assert_eq!(mandate_after.amount, fixture.plan_a_state.amount);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before
    );
}
