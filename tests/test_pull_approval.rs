#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::{SubscriptionFixture, TestHarness};
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{VelaMandate, VelaPlan},
};

fn setup_fixture() -> (
    TestHarness,
    SubscriptionFixture,
    VelaPlan,
    VelaMandate,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    let wrapped_usdc_mint = fixture.wrapped_usdc_mint;

    let admin = harness.merchant.insecure_clone();

    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let subscriber_usdc =
        harness.create_spl_token_account(&fixture.subscriber, &fixture.usdc_mint, &subscriber);
    harness.mint_spl_tokens(
        &admin,
        &fixture.usdc_mint,
        &subscriber_usdc,
        plan.amount * 10,
    );

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
            plan.amount * 2,
        )
        .expect("wrap into the mandate billing account should succeed");

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

#[test]
fn test_pull_with_valid_approval() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture();
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
        harness
            .svm
            .get_account(&helpers::to_address(approval))
            .is_none(),
        "approval PDA should be closed after a successful pull",
    );
}

#[test]
fn test_pull_with_expired_approval_fails() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture();
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
