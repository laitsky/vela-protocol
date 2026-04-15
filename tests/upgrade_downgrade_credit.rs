#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use upgrade_helpers::{
    assert_single_event, assert_upgrade_events, fetch_mandate, setup_periodic_upgrade_fixture,
    wrapped_balance, PERIOD_SECONDS,
};
use vela_protocol::state::MandateCreditAdded;

#[test]
fn test_update_mandate_plan_turns_downgrade_into_credit_balance() {
    let mut fixture = setup_periodic_upgrade_fixture(20_000_000, 10_000_000);
    let half_period = (PERIOD_SECONDS / 2) as i64;
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due - half_period);

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
        .expect("downgrade should succeed");

    assert_upgrade_events(&metadata, fixture.mandate, fixture.plan_a, fixture.plan_b, -5_000_000);
    let credit_event = assert_single_event::<MandateCreditAdded>(&metadata);
    assert_eq!(credit_event.credit_amount, 5_000_000);
    assert_eq!(credit_event.new_credit_balance, 5_000_000);

    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.plan, fixture.plan_b);
    assert_eq!(mandate_after.credit_balance, 5_000_000);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before
    );
}
