#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{SubscriptionFixture, TestHarness};
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{VelaMandate, VelaPlan},
};

fn setup_fixture() -> (TestHarness, SubscriptionFixture, VelaPlan, VelaMandate) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    (harness, fixture, plan, mandate)
}

#[test]
fn test_pull_with_valid_approval() {
    let (mut harness, fixture, plan, mandate_before) = setup_fixture();
    harness.set_clock_timestamp(mandate_before.next_payment_due);
    let approval = harness.create_pull_approval(&fixture.mandate, mandate_before.next_payment_due, true);

    harness
        .send_execute_pull(
            &fixture.subscriber,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect("execute_pull should consume a valid approval");

    assert!(
        harness.svm.get_account(&helpers::to_address(approval)).is_none(),
        "approval PDA should be closed after a successful pull",
    );
}

#[test]
fn test_pull_with_expired_approval_fails() {
    let (mut harness, fixture, plan, mandate_before) = setup_fixture();
    harness.set_clock_timestamp(mandate_before.next_payment_due + 1);
    harness.create_pull_approval(&fixture.mandate, mandate_before.next_payment_due, true);

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("expired approvals should be rejected");

    assert!(
        format!("{:?}", error.err).contains("Custom(6012)"),
        "expected ApprovalExpired custom error, got {:?}",
        error.err,
    );
}
