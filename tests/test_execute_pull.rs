#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{SubscriptionFixture, TestHarness};
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{MandateStatus, VelaMandate, VelaPlan},
};
use anchor_lang::prelude::Pubkey;

/// Set up a full fixture with Token-2022 wrapped USDC accounts injected.
/// The mandate PDA is used as authority over the subscriber's wrapped account.
fn setup_fixture() -> (TestHarness, SubscriptionFixture, VelaPlan, VelaMandate, Pubkey, Pubkey, Pubkey) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);

    // Inject Token-2022 wrapped USDC mint
    let wrapped_mint = Keypair::new();
    let wrapped_mint_pubkey = helpers::to_anchor_pubkey(wrapped_mint.pubkey());
    let (mint_authority, _) = harness.derive_mint_authority();
    harness.inject_token_2022_mint(&wrapped_mint_pubkey, &mint_authority, 1_000_000_000);

    // Inject subscriber's wrapped USDC account (mandate PDA is authority)
    let subscriber_wrapped = Keypair::new();
    let subscriber_wrapped_pubkey = helpers::to_anchor_pubkey(subscriber_wrapped.pubkey());
    harness.inject_token_2022_account(
        &subscriber_wrapped_pubkey,
        &wrapped_mint_pubkey,
        &fixture.mandate,
        plan.amount * 10,
    );

    // Inject merchant's wrapped USDC account
    let merchant_wrapped = Keypair::new();
    let merchant_wrapped_pubkey = helpers::to_anchor_pubkey(merchant_wrapped.pubkey());
    harness.inject_token_2022_account(
        &merchant_wrapped_pubkey,
        &wrapped_mint_pubkey,
        &harness.merchant_pubkey(),
        0,
    );

    (harness, fixture, plan, mandate, subscriber_wrapped_pubkey, merchant_wrapped_pubkey, wrapped_mint_pubkey)
}

#[test]
fn test_execute_pull_success() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) = setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    let meta = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
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
}

#[test]
fn test_execute_pull_permissionless() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) = setup_fixture();
    let payer = harness.create_wallet();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    harness
        .send_execute_pull(
            &payer,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("third-party payer should be able to execute_pull");

    let mandate_after: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert_eq!(mandate_after.pulls_executed, 1);
}

#[test]
fn test_execute_pull_too_early() {
    let (mut harness, fixture, plan, _mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) = setup_fixture();
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
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
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
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
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) = setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    let meta = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
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
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) = setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
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
        .expect("first execute_pull should succeed");

    let mandate_after_first: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.set_clock_timestamp(mandate_after_first.next_payment_due);
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
        .expect_err("second pull should be blocked until billing is finalized");

    assert!(
        format!("{:?}", error.err).contains("Custom(6019)"),
        "expected PendingBillingRecord custom error, got {:?}",
        error.err,
    );
}
