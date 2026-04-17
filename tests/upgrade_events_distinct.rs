#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use upgrade_helpers::{
    assert_cancel_event, assert_no_event, assert_single_event, assert_upgrade_events,
    fetch_mandate, send_periodic_execute_pull, setup_periodic_upgrade_fixture,
};
use vela_protocol::constants::WRAPPED_USDC_SYMBOL;
use vela_protocol::state::{
    MandateCreditAdded, MandateUpgradeCancelled, MandateUpgradeFinalized, MandateUpgradeInitiated,
};

const BASIC: u64 = 10_000_000;
const PREMIUM: u64 = 20_000_000;

#[test]
fn immediate_upgrade_emits_only_initiated_and_finalized_events() {
    let mut fixture = setup_periodic_upgrade_fixture(BASIC, PREMIUM);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    let halfway = mandate_before.start_date
        + ((mandate_before.next_payment_due - mandate_before.start_date) / 2);

    fixture.harness.set_clock_timestamp(halfway);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        BASIC,
    );

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
        .expect("upgrade should succeed");

    assert_upgrade_events(
        &metadata,
        fixture.mandate,
        fixture.plan_a,
        fixture.plan_b,
        5_000_000,
    );
    let initiated = assert_single_event::<MandateUpgradeInitiated>(&metadata);
    let finalized = assert_single_event::<MandateUpgradeFinalized>(&metadata);
    assert_eq!(initiated.mint, fixture.wrapped_mint);
    assert_eq!(initiated.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_eq!(initiated.change_type, 1);
    assert_eq!(finalized.mint, fixture.wrapped_mint);
    assert_eq!(finalized.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_eq!(finalized.change_type, 1);
    assert_no_event::<MandateCreditAdded>(&metadata);
    assert_no_event::<MandateUpgradeCancelled>(&metadata);
}

#[test]
fn immediate_downgrade_adds_credit_without_cancel_event() {
    let mut fixture = setup_periodic_upgrade_fixture(PREMIUM, BASIC);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    let halfway = mandate_before.start_date
        + ((mandate_before.next_payment_due - mandate_before.start_date) / 2);

    fixture.harness.set_clock_timestamp(halfway);

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

    assert_upgrade_events(
        &metadata,
        fixture.mandate,
        fixture.plan_a,
        fixture.plan_b,
        -5_000_000,
    );
    let credit = assert_single_event::<MandateCreditAdded>(&metadata);
    assert_eq!(credit.mandate, fixture.mandate);
    assert_eq!(credit.old_plan, fixture.plan_a);
    assert_eq!(credit.new_plan, fixture.plan_b);
    assert_eq!(credit.credit_amount, 5_000_000);
    assert_eq!(credit.mint, fixture.wrapped_mint);
    assert_eq!(credit.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_no_event::<MandateUpgradeCancelled>(&metadata);
}

#[test]
fn schedule_only_emits_initiated_event() {
    let mut fixture = setup_periodic_upgrade_fixture(BASIC, PREMIUM);

    let metadata = fixture
        .harness
        .send_schedule_plan_change(&fixture.subscriber, &fixture.mandate, &fixture.plan_b)
        .expect("schedule should succeed");

    let initiated = assert_single_event::<MandateUpgradeInitiated>(&metadata);
    assert_eq!(initiated.mandate, fixture.mandate);
    assert_eq!(initiated.old_plan, fixture.plan_a);
    assert_eq!(initiated.new_plan, fixture.plan_b);
    assert_eq!(initiated.proration_amount, 0);
    assert_eq!(initiated.change_type, 2);
    assert_eq!(initiated.mint, fixture.wrapped_mint);
    assert_eq!(initiated.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_no_event::<MandateUpgradeFinalized>(&metadata);
    assert_no_event::<MandateCreditAdded>(&metadata);
    assert_no_event::<MandateUpgradeCancelled>(&metadata);
}

#[test]
fn cancel_scheduled_change_emits_only_cancelled_event() {
    let mut fixture = setup_periodic_upgrade_fixture(BASIC, PREMIUM);

    fixture
        .harness
        .send_schedule_plan_change(&fixture.subscriber, &fixture.mandate, &fixture.plan_b)
        .expect("schedule should succeed");
    let metadata = fixture
        .harness
        .send_cancel_plan_change(&fixture.subscriber, &fixture.mandate)
        .expect("cancel should succeed");

    assert_cancel_event(&metadata, fixture.mandate, fixture.plan_a, fixture.plan_b);
    assert_no_event::<MandateUpgradeInitiated>(&metadata);
    assert_no_event::<MandateUpgradeFinalized>(&metadata);
    assert_no_event::<MandateCreditAdded>(&metadata);
}

#[test]
fn auto_applied_schedule_emits_only_finalized_event_on_pull() {
    let mut fixture = setup_periodic_upgrade_fixture(BASIC, PREMIUM);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);

    fixture
        .harness
        .send_schedule_plan_change(&fixture.subscriber, &fixture.mandate, &fixture.plan_b)
        .expect("schedule should succeed");
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        PREMIUM,
    );
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due);
    let payer = fixture.subscriber.insecure_clone();
    let current_plan = fixture.plan_a;
    let pending_plan = fixture.plan_b;

    let metadata =
        send_periodic_execute_pull(&mut fixture, &payer, &current_plan, Some(&pending_plan))
            .expect("pull should succeed");

    let finalized = assert_single_event::<MandateUpgradeFinalized>(&metadata);
    assert_eq!(finalized.mandate, fixture.mandate);
    assert_eq!(finalized.old_plan, fixture.plan_a);
    assert_eq!(finalized.new_plan, fixture.plan_b);
    assert_eq!(finalized.proration_amount, 0);
    assert_eq!(finalized.change_type, 2);
    assert_eq!(finalized.mint, fixture.wrapped_mint);
    assert_eq!(finalized.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_no_event::<MandateUpgradeInitiated>(&metadata);
    assert_no_event::<MandateCreditAdded>(&metadata);
    assert_no_event::<MandateUpgradeCancelled>(&metadata);
}
