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
fn test_pull_fails_without_approval() {
    let (mut harness, fixture, plan, mandate) = setup_fixture();
    harness.set_clock_timestamp(mandate.next_payment_due);

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            plan.plan_id,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
        )
        .expect_err("execute_pull should fail closed without a PullApproval");

    assert!(
        format!("{:?}", error.err).contains("Custom(6011)"),
        "expected ApprovalNotGranted custom error, got {:?}",
        error.err,
    );

    let merchant_token = harness.fetch_spl_token_account(&fixture.merchant_token_account);
    assert_eq!(merchant_token.amount, 0, "merchant balance should stay unchanged");
}
