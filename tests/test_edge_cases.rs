#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::{SubscriptionFixture, TestHarness};
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{MandateStatus, VelaMandate, VelaPlan},
};

/// Set up a subscription fixture with Token-2022 wrapped USDC accounts.
fn setup_fixture(
    amount: u64,
    frequency: u64,
    trial_period: u64,
    max_pulls: u64,
) -> (
    TestHarness,
    SubscriptionFixture,
    VelaPlan,
    VelaMandate,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(amount, frequency, trial_period, max_pulls);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    let wrapped_usdc_mint = fixture.wrapped_usdc_mint;

    let admin = harness.merchant.insecure_clone();
    let subscriber = subscriber_pubkey(&fixture);
    let subscriber_usdc =
        harness.create_spl_token_account(&fixture.subscriber, &fixture.usdc_mint, &subscriber);
    harness.mint_spl_tokens(&admin, &fixture.usdc_mint, &subscriber_usdc, amount * 10);

    let subscriber_wrapped_pubkey =
        harness.create_token_2022_ata(&admin, &fixture.mandate, &fixture.wrapped_usdc_mint);
    harness
        .send_wrap(
            &fixture.subscriber,
            &fixture.usdc_mint,
            &fixture.wrapped_usdc_mint,
            &subscriber_usdc,
            &subscriber_wrapped_pubkey,
            &fixture.mandate,
            &fixture.wrapping_vault,
            amount * 10,
        )
        .expect("wrap into mandate billing account should succeed");

    let merchant_wrapped_pubkey = harness.create_token_2022_ata(
        &admin,
        &harness.merchant_pubkey(),
        &fixture.wrapped_usdc_mint,
    );

    (
        harness,
        fixture,
        plan,
        mandate,
        subscriber_wrapped_pubkey,
        merchant_wrapped_pubkey,
        wrapped_usdc_mint,
    )
}

fn subscriber_pubkey(fixture: &SubscriptionFixture) -> Pubkey {
    Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes())
}

fn assert_custom_error(error: &litesvm::types::FailedTransactionMetadata, code: u32, label: &str) {
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
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let subscriber = subscriber_pubkey(&fixture);

    harness.set_clock_timestamp(mandate.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        plan.amount,
    );
    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("first pull should succeed");

    finalize_billing_for_current_pull(&mut harness, &fixture.mandate);
    let mandate_after_first: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.set_clock_timestamp(mandate_after_first.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_after_first.next_payment_due,
        true,
        plan.amount,
    );
    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("second pull should succeed");

    let mandate_after_second: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert!(matches!(
        mandate_after_second.status,
        MandateStatus::Expired
    ));

    harness.set_clock_timestamp(mandate_after_second.next_payment_due);
    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("expired mandate should reject additional pulls");

    assert_custom_error(&error, 6001, "MandateNotActive");
}

#[test]
fn test_early_pull_fails() {
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture(25_000_000, 86_400, 0, 4);
    let subscriber = subscriber_pubkey(&fixture);

    harness.set_clock_timestamp(mandate.start_date + 100);
    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("pulls before next_payment_due should fail");

    assert_custom_error(&error, 6000, "PullTooEarly");
}

#[test]
fn test_double_pull_same_period_fails() {
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture(25_000_000, 2_592_000, 0, 4);
    let subscriber = subscriber_pubkey(&fixture);

    harness.set_clock_timestamp(mandate.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        plan.amount,
    );
    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("first pull should succeed");

    finalize_billing_for_current_pull(&mut harness, &fixture.mandate);
    let mandate_after_first: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_after_first.next_payment_due,
        true,
        plan.amount,
    );
    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("second pull in the same period should fail");

    assert_custom_error(&error, 6000, "PullTooEarly");
}

#[test]
fn test_insufficient_balance_pull_fails() {
    // With Token-2022, insufficient balance causes the transfer_checked to fail.
    // Inject an account with 0 balance to trigger a Token-2022 transfer error.
    let (mut harness, fixture, plan, mandate, _sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture(1_000_000, MIN_FREQUENCY_SECONDS, 0, 3);
    let subscriber = subscriber_pubkey(&fixture);

    // Create a separate mandate-owned wrapped account with 0 balance.
    let empty_wrapped = Keypair::new();
    let empty_wrapped_pubkey = helpers::to_anchor_pubkey(empty_wrapped.pubkey());
    harness.inject_token_2022_account(&empty_wrapped_pubkey, &wrapped_mint, &fixture.mandate, 0);

    harness.set_clock_timestamp(mandate.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        plan.amount,
    );

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &empty_wrapped_pubkey,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("pull with zero balance should fail");

    // Token-2022 returns InsufficientFunds or similar when balance is too low
    let err_string = format!("{:?}", error.err);
    assert!(
        err_string.contains("Custom(") || err_string.contains("InsufficientFunds"),
        "expected insufficient funds error, got {:?}",
        error.err,
    );
}

#[test]
fn test_unauthorized_cancel_fails() {
    let (mut harness, fixture, plan, _, _, _, _) =
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
    let (mut harness, fixture, plan, _, _, _, _) =
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
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
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
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("cancelled mandates should reject pulls");

    assert_custom_error(&error, 6001, "MandateNotActive");
}

#[test]
fn test_expiry_date_passed() {
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture(
            25_000_000,
            MIN_FREQUENCY_SECONDS,
            MIN_FREQUENCY_SECONDS,
            100,
        );
    let subscriber = subscriber_pubkey(&fixture);

    assert!(mandate.expiry > 0, "fixture should set an expiry");
    harness.set_clock_timestamp(mandate.expiry + 1);

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("mandates past expiry should reject pulls");

    assert_custom_error(&error, 6008, "MandateExpired");
}
