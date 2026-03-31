#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{SubscriptionFixture, TestHarness};
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{MandateStatus, VelaMandate, VelaPlan},
};

fn setup_fixture() -> (TestHarness, SubscriptionFixture, VelaPlan, VelaMandate) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    (harness, fixture, plan, mandate)
}

#[test]
fn test_execute_pull_success() {
    let (mut harness, fixture, plan, mandate_before) = setup_fixture();
    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval(&fixture.mandate, mandate_before.next_payment_due, true);

    let meta = harness
        .send_execute_pull(
            &fixture.subscriber,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect("execute_pull should succeed");

    assert!(meta.compute_units_consumed > 0);

    let mandate_after: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert_eq!(mandate_after.pulls_executed, 1);
    assert_eq!(
        mandate_after.next_payment_due,
        mandate_before.next_payment_due + plan.frequency as i64
    );
    assert!(matches!(mandate_after.status, MandateStatus::Active));

    let subscriber_token = harness.fetch_spl_token_account(&fixture.subscriber_token_account);
    let merchant_token = harness.fetch_spl_token_account(&fixture.merchant_token_account);
    assert_eq!(subscriber_token.amount, 25_000_000 * plan.max_pulls);
    assert_eq!(merchant_token.amount, plan.amount);
}

#[test]
fn test_execute_pull_permissionless() {
    let (mut harness, fixture, plan, mandate_before) = setup_fixture();
    let payer = harness.create_wallet();
    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval(&fixture.mandate, mandate_before.next_payment_due, true);

    harness
        .send_execute_pull(
            &payer,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect("third-party payer should be able to execute_pull");

    let merchant_token = harness.fetch_spl_token_account(&fixture.merchant_token_account);
    assert_eq!(merchant_token.amount, plan.amount);
}

#[test]
fn test_execute_pull_too_early() {
    let (mut harness, fixture, plan, _mandate_before) = setup_fixture();
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.create_pull_approval(&fixture.mandate, mandate.next_payment_due, true);

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("execute_pull should reject early pulls");

    assert!(
        format!("{:?}", error.err).contains("Custom(6000)"),
        "expected PullTooEarly custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_execute_pull_cu_budget() {
    let (mut harness, fixture, plan, mandate_before) = setup_fixture();
    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval(&fixture.mandate, mandate_before.next_payment_due, true);

    let meta = harness
        .send_execute_pull(
            &fixture.subscriber,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect("execute_pull should succeed");

    assert!(
        meta.compute_units_consumed < 150_000,
        "execute_pull CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}

#[test]
fn test_next_pull_requires_billing_record_finalization() {
    let (mut harness, fixture, plan, mandate_before) = setup_fixture();
    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval(&fixture.mandate, mandate_before.next_payment_due, true);

    harness
        .send_execute_pull(
            &fixture.subscriber,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect("first execute_pull should succeed");

    let mandate_after_first: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.set_clock_timestamp(mandate_after_first.next_payment_due);
    harness.create_pull_approval(
        &fixture.mandate,
        mandate_after_first.next_payment_due,
        true,
    );

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("second pull should be blocked until billing is finalized");

    assert!(
        format!("{:?}", error.err).contains("Custom(6019)"),
        "expected PendingBillingRecord custom error, got {:?}",
        error.err,
    );
}
