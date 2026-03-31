#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{SubscriptionFixture, TestHarness};
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{VelaMandate, VelaPlan},
};
use anchor_lang::prelude::Pubkey;

fn setup_fixture() -> (TestHarness, SubscriptionFixture, VelaPlan, VelaMandate, Pubkey, Pubkey, Pubkey) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);

    let wrapped_mint = Keypair::new();
    let wrapped_mint_pubkey = helpers::to_anchor_pubkey(wrapped_mint.pubkey());
    let (mint_authority, _) = harness.derive_mint_authority();
    harness.inject_token_2022_mint(&wrapped_mint_pubkey, &mint_authority, 1_000_000_000);

    let subscriber_wrapped = Keypair::new();
    let subscriber_wrapped_pubkey = helpers::to_anchor_pubkey(subscriber_wrapped.pubkey());
    harness.inject_token_2022_account(
        &subscriber_wrapped_pubkey,
        &wrapped_mint_pubkey,
        &fixture.mandate,
        plan.amount * 10,
    );

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
fn test_pull_with_valid_approval() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) = setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    let approval = harness.create_pull_approval_with_amount(
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
        .expect("execute_pull should consume a valid approval");

    assert!(
        harness.svm.get_account(&helpers::to_address(approval)).is_none(),
        "approval PDA should be closed after a successful pull",
    );
}

#[test]
fn test_pull_with_expired_approval_fails() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) = setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    // Set clock past valid_until so approval is expired
    harness.set_clock_timestamp(mandate_before.next_payment_due + 1);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
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
        .expect_err("expired approvals should be rejected");

    assert!(
        format!("{:?}", error.err).contains("Custom(6012)"),
        "expected ApprovalExpired custom error, got {:?}",
        error.err,
    );
}
