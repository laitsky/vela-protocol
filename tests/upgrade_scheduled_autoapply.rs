#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use upgrade_helpers::{
    assert_custom_error, assert_single_event, fetch_mandate, send_periodic_execute_pull,
    setup_periodic_upgrade_fixture, wrapped_balance,
};
use vela_protocol::state::{MandateUpgradeFinalized, VelaMandate};

const PERIODIC_BASIC_AMOUNT: u64 = 10_000_000;
const PERIODIC_PREMIUM_AMOUNT: u64 = 20_000_000;

fn overwrite_credit_balance(
    fixture: &mut upgrade_helpers::PeriodicUpgradeFixture,
    credit_balance: u64,
) -> VelaMandate {
    let mut mandate = fetch_mandate(&fixture.harness, &fixture.mandate);
    mandate.credit_balance = credit_balance;
    fixture
        .harness
        .overwrite_anchor_account(&fixture.mandate, &mandate);
    mandate
}

fn halfway_timestamp(mandate: &VelaMandate) -> i64 {
    mandate.start_date + ((mandate.next_payment_due - mandate.start_date) / 2)
}

#[test]
fn scheduled_upgrade_auto_applies_on_next_due_pull() {
    let mut fixture =
        setup_periodic_upgrade_fixture(PERIODIC_BASIC_AMOUNT, PERIODIC_PREMIUM_AMOUNT);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);

    fixture
        .harness
        .send_schedule_plan_change(&fixture.subscriber, &fixture.mandate, &fixture.plan_b)
        .expect("schedule should succeed");
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        PERIODIC_PREMIUM_AMOUNT,
    );
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due);

    let subscriber_before = wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped);
    let merchant_before = wrapped_balance(&fixture.harness, &fixture.merchant_wrapped);
    let payer = fixture.subscriber.insecure_clone();
    let current_plan = fixture.plan_a;
    let pending_plan = fixture.plan_b;

    let metadata =
        send_periodic_execute_pull(&mut fixture, &payer, &current_plan, Some(&pending_plan))
            .expect("execute pull with pending plan should succeed");

    let finalized = assert_single_event::<MandateUpgradeFinalized>(&metadata);
    assert_eq!(finalized.mandate, fixture.mandate);
    assert_eq!(finalized.old_plan, fixture.plan_a);
    assert_eq!(finalized.new_plan, fixture.plan_b);
    assert_eq!(finalized.proration_amount, 0);
    assert_eq!(finalized.change_type, 2);

    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.plan, fixture.plan_b);
    assert_eq!(mandate_after.amount, PERIODIC_PREMIUM_AMOUNT);
    assert_eq!(mandate_after.credit_balance, 0);
    assert_eq!(
        mandate_after.pending_new_plan,
        anchor_lang::prelude::Pubkey::default()
    );
    assert_eq!(mandate_after.pending_change_type, 0);
    assert_eq!(mandate_after.pulls_executed, 1);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before - PERIODIC_PREMIUM_AMOUNT
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before + PERIODIC_PREMIUM_AMOUNT
    );
}

#[test]
fn scheduled_upgrade_requires_pending_plan_account_at_execution() {
    let mut fixture =
        setup_periodic_upgrade_fixture(PERIODIC_BASIC_AMOUNT, PERIODIC_PREMIUM_AMOUNT);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);

    fixture
        .harness
        .send_schedule_plan_change(&fixture.subscriber, &fixture.mandate, &fixture.plan_b)
        .expect("schedule should succeed");
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        PERIODIC_PREMIUM_AMOUNT,
    );
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due);
    let payer = fixture.subscriber.insecure_clone();

    let current_plan = fixture.plan_a;
    let failure = send_periodic_execute_pull(&mut fixture, &payer, &current_plan, None)
        .expect_err("missing pending plan account must fail");

    assert_custom_error(&failure, 6715);
}

#[test]
fn downgrade_credit_offsets_next_periodic_pull() {
    let mut fixture =
        setup_periodic_upgrade_fixture(PERIODIC_PREMIUM_AMOUNT, PERIODIC_BASIC_AMOUNT);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    let halfway = halfway_timestamp(&mandate_before);

    fixture.harness.set_clock_timestamp(halfway);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        PERIODIC_BASIC_AMOUNT,
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
        .expect("downgrade should succeed");

    let after_downgrade = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(after_downgrade.credit_balance, PERIODIC_BASIC_AMOUNT / 2);
    fixture
        .harness
        .set_clock_timestamp(after_downgrade.next_payment_due);

    let subscriber_before = wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped);
    let merchant_before = wrapped_balance(&fixture.harness, &fixture.merchant_wrapped);
    let payer = fixture.subscriber.insecure_clone();

    let current_plan = fixture.plan_b;
    send_periodic_execute_pull(&mut fixture, &payer, &current_plan, None)
        .expect("pull after downgrade should succeed");

    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.credit_balance, 0);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before - (PERIODIC_BASIC_AMOUNT / 2)
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before + (PERIODIC_BASIC_AMOUNT / 2)
    );
}

#[test]
fn oversized_credit_zeroes_charge_without_transfer() {
    let mut fixture =
        setup_periodic_upgrade_fixture(PERIODIC_PREMIUM_AMOUNT, PERIODIC_BASIC_AMOUNT);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    let halfway = halfway_timestamp(&mandate_before);

    fixture.harness.set_clock_timestamp(halfway);
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
        .expect("downgrade should succeed");

    let mandate = overwrite_credit_balance(&mut fixture, 50_000_000);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due + 600,
        true,
        PERIODIC_BASIC_AMOUNT,
    );
    fixture
        .harness
        .set_clock_timestamp(mandate.next_payment_due);

    let subscriber_before = wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped);
    let merchant_before = wrapped_balance(&fixture.harness, &fixture.merchant_wrapped);
    let payer = fixture.subscriber.insecure_clone();

    let current_plan = fixture.plan_b;
    send_periodic_execute_pull(&mut fixture, &payer, &current_plan, None)
        .expect("zero-charge pull should succeed");

    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.credit_balance, 40_000_000);
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
fn auto_apply_consumes_existing_credit_after_plan_switch() {
    let mut fixture =
        setup_periodic_upgrade_fixture(PERIODIC_BASIC_AMOUNT, PERIODIC_PREMIUM_AMOUNT);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);

    fixture
        .harness
        .send_schedule_plan_change(&fixture.subscriber, &fixture.mandate, &fixture.plan_b)
        .expect("schedule should succeed");
    overwrite_credit_balance(&mut fixture, 5_000_000);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        PERIODIC_PREMIUM_AMOUNT,
    );
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due);

    let subscriber_before = wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped);
    let merchant_before = wrapped_balance(&fixture.harness, &fixture.merchant_wrapped);
    let payer = fixture.subscriber.insecure_clone();
    let current_plan = fixture.plan_a;
    let pending_plan = fixture.plan_b;

    send_periodic_execute_pull(&mut fixture, &payer, &current_plan, Some(&pending_plan))
        .expect("pull with scheduled upgrade and prior credit should succeed");

    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.plan, fixture.plan_b);
    assert_eq!(mandate_after.credit_balance, 0);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before - 15_000_000
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before + 15_000_000
    );
}
