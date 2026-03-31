#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::{SubscriptionFixture, TestHarness};
use solana_pubkey::Pubkey as SplPubkey;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{MandateStatus, VelaMandate, VelaPlan},
};

fn setup_fixture(
    amount: u64,
    frequency: u64,
    trial_period: u64,
    max_pulls: u64,
) -> (TestHarness, SubscriptionFixture, VelaPlan, VelaMandate) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(amount, frequency, trial_period, max_pulls);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    (harness, fixture, plan, mandate)
}

fn subscriber_pubkey(fixture: &SubscriptionFixture) -> Pubkey {
    Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes())
}

fn assert_custom_error(
    error: &litesvm::types::FailedTransactionMetadata,
    code: u32,
    label: &str,
) {
    assert!(
        format!("{:?}", error.err).contains(&format!("Custom({code})")),
        "expected {label} custom error {code}, got {:?}",
        error.err,
    );
}

fn finalize_billing_for_current_pull(harness: &mut TestHarness, mandate_pubkey: &Pubkey) {
    let mut mandate: VelaMandate = harness.fetch_anchor_account(mandate_pubkey);
    mandate.last_billing_recorded_pull = mandate.pulls_executed;
    harness.overwrite_anchor_account(mandate_pubkey, &mandate);
}

#[test]
fn test_expired_mandate_pull_fails() {
    let (mut harness, fixture, plan, mandate) =
        setup_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let subscriber = subscriber_pubkey(&fixture);

    harness.set_clock_timestamp(mandate.next_payment_due);
    harness.create_pull_approval(&fixture.mandate, mandate.next_payment_due, true);
    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect("first pull should succeed");

    finalize_billing_for_current_pull(&mut harness, &fixture.mandate);
    let mandate_after_first: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.set_clock_timestamp(mandate_after_first.next_payment_due);
    harness.create_pull_approval(
        &fixture.mandate,
        mandate_after_first.next_payment_due,
        true,
    );
    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect("second pull should succeed");

    let mandate_after_second: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert!(matches!(mandate_after_second.status, MandateStatus::Expired));

    harness.set_clock_timestamp(mandate_after_second.next_payment_due);
    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("expired mandate should reject additional pulls");

    assert_custom_error(&error, 6001, "MandateNotActive");
}

#[test]
fn test_early_pull_fails() {
    let (mut harness, fixture, plan, mandate) =
        setup_fixture(25_000_000, 86_400, 0, 4);
    let subscriber = subscriber_pubkey(&fixture);

    harness.set_clock_timestamp(mandate.start_date + 100);
    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("pulls before next_payment_due should fail");

    assert_custom_error(&error, 6000, "PullTooEarly");
}

#[test]
fn test_double_pull_same_period_fails() {
    let (mut harness, fixture, plan, mandate) =
        setup_fixture(25_000_000, 2_592_000, 0, 4);
    let subscriber = subscriber_pubkey(&fixture);

    harness.set_clock_timestamp(mandate.next_payment_due);
    harness.create_pull_approval(&fixture.mandate, mandate.next_payment_due, true);
    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect("first pull should succeed");

    finalize_billing_for_current_pull(&mut harness, &fixture.mandate);
    let mandate_after_first: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.create_pull_approval(
        &fixture.mandate,
        mandate_after_first.next_payment_due,
        true,
    );
    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("second pull in the same period should fail");

    assert_custom_error(&error, 6000, "PullTooEarly");
}

#[test]
fn test_insufficient_balance_pull_fails() {
    let (mut harness, fixture, plan, mandate) =
        setup_fixture(1_000_000, MIN_FREQUENCY_SECONDS, 0, 3);
    let subscriber = subscriber_pubkey(&fixture);
    let drain_wallet = harness.create_wallet();
    let drain_owner = Pubkey::new_from_array(drain_wallet.pubkey().to_bytes());
    let drain_token_account =
        harness.create_spl_token_account(&fixture.subscriber, &fixture.usdc_mint, &drain_owner);
    let subscriber_balance = harness
        .fetch_spl_token_account(&fixture.subscriber_token_account)
        .amount;

    let drain_ix = spl_token::instruction::transfer_checked(
        &SplPubkey::new_from_array(spl_token::id().to_bytes()),
        &SplPubkey::new_from_array(fixture.subscriber_token_account.to_bytes()),
        &SplPubkey::new_from_array(fixture.usdc_mint.to_bytes()),
        &SplPubkey::new_from_array(drain_token_account.to_bytes()),
        &SplPubkey::new_from_array(subscriber.to_bytes()),
        &[],
        subscriber_balance,
        6,
    )
    .expect("drain transfer instruction should build");
    harness
        .send_instructions(
            &[drain_ix],
            &[&fixture.subscriber],
            Some(&fixture.subscriber.pubkey()),
        )
        .expect("subscriber should be able to drain funds");

    harness.set_clock_timestamp(mandate.next_payment_due);
    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("pull with zero balance should fail");

    let err_string = format!("{:?}", error.err);
    assert!(
        err_string.contains("Custom(6003)") || err_string.contains("InsufficientFunds"),
        "expected protocol or SPL insufficient funds error, got {:?}",
        error.err,
    );
}

#[test]
fn test_unauthorized_cancel_fails() {
    let (mut harness, fixture, plan, _) =
        setup_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 3);
    let attacker = harness.create_wallet();
    let subscriber = subscriber_pubkey(&fixture);

    let error = harness
        .send_cancel(
            &attacker,
            &subscriber,
            plan.plan_id,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect_err("non-subscriber cancel should fail");

    assert_custom_error(&error, 6004, "UnauthorizedCancel");
}

#[test]
fn test_cancel_already_cancelled_fails() {
    let (mut harness, fixture, plan, _) =
        setup_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 3);
    let subscriber = subscriber_pubkey(&fixture);

    harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect("first cancel should succeed");

    let error = harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect_err("second cancel should fail");

    assert_custom_error(&error, 6001, "MandateNotActive");
}

#[test]
fn test_pull_after_cancel_fails() {
    let (mut harness, fixture, plan, mandate) =
        setup_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 3);
    let subscriber = subscriber_pubkey(&fixture);

    harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect("cancel should succeed");

    harness.set_clock_timestamp(mandate.next_payment_due);
    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("cancelled mandates should reject pulls");

    assert_custom_error(&error, 6001, "MandateNotActive");
}

#[test]
fn test_expiry_date_passed() {
    let (mut harness, fixture, plan, mandate) =
        setup_fixture(25_000_000, MIN_FREQUENCY_SECONDS, MIN_FREQUENCY_SECONDS, 100);
    let subscriber = subscriber_pubkey(&fixture);

    assert!(mandate.expiry > 0, "fixture should set an expiry");
    harness.set_clock_timestamp(mandate.expiry + 1);

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("mandates past expiry should reject pulls");

    assert_custom_error(&error, 6008, "MandateExpired");
}
