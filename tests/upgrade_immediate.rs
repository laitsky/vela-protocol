#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use upgrade_helpers::{
    assert_upgrade_events, fetch_mandate, setup_periodic_upgrade_fixture, wrapped_balance,
    PERIOD_SECONDS,
};

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

    assert_upgrade_events(&metadata, fixture.mandate, fixture.plan_a, fixture.plan_b, 5_000_000);

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
}
