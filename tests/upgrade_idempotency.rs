#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use upgrade_helpers::{
    assert_cancel_event, assert_no_event, assert_single_event, assert_upgrade_events,
    fetch_mandate, setup_periodic_upgrade_fixture, wrapped_balance, PERIOD_SECONDS,
};
use vela_protocol::state::{MandateUpgradeFinalized, MandateUpgradeInitiated};

#[test]
fn test_immediate_upgrade_is_idempotent_after_first_success() {
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

    let first = fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("first upgrade should succeed");
    let second = fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("same-plan retry should noop");

    assert_upgrade_events(
        &first,
        fixture.mandate,
        fixture.plan_a,
        fixture.plan_b,
        5_000_000,
    );
    assert_no_event::<MandateUpgradeInitiated>(&second);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before - 5_000_000
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before + 5_000_000
    );
}

#[test]
fn test_schedule_plan_change_is_same_slot_idempotent_and_cancel_clears_pending() {
    let mut fixture = setup_periodic_upgrade_fixture(10_000_000, 20_000_000);
    let now = fetch_mandate(&fixture.harness, &fixture.mandate).start_date + 60;
    fixture.harness.set_clock_timestamp(now);

    let first = fixture
        .harness
        .send_schedule_plan_change(&fixture.subscriber, &fixture.mandate, &fixture.plan_b)
        .expect("first scheduled change should succeed");
    let second = fixture
        .harness
        .send_schedule_plan_change(&fixture.subscriber, &fixture.mandate, &fixture.plan_b)
        .expect("same-slot retry should noop");

    let after_schedule = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(after_schedule.pending_new_plan, fixture.plan_b);
    assert_eq!(after_schedule.pending_change_type, 2);
    assert_eq!(after_schedule.pending_nonce_short, 0u64.to_le_bytes());
    let initiated = assert_single_event::<MandateUpgradeInitiated>(&first);
    assert_eq!(initiated.mandate, fixture.mandate);
    assert_eq!(initiated.old_plan, fixture.plan_a);
    assert_eq!(initiated.new_plan, fixture.plan_b);
    assert_eq!(initiated.proration_amount, 0);
    assert_no_event::<MandateUpgradeFinalized>(&first);
    assert_no_event::<MandateUpgradeInitiated>(&second);

    let cancelled = fixture
        .harness
        .send_cancel_plan_change(&fixture.subscriber, &fixture.mandate)
        .expect("cancel should succeed");
    assert_cancel_event(&cancelled, fixture.mandate, fixture.plan_a, fixture.plan_b);

    let after_cancel = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(
        after_cancel.pending_new_plan,
        anchor_lang::prelude::Pubkey::default()
    );
    assert_eq!(after_cancel.pending_change_type, 0);
}
