#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{SubscriptionFixture, TestHarness};
use solana_program_pack::Pack;
use solana_signer::Signer;
use spl_token_2022::state::Account as Token2022Account;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{MandateStatus, VelaMandate, VelaPlan},
};

fn setup_fixture() -> (TestHarness, SubscriptionFixture, VelaPlan) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    (harness, fixture, plan)
}

#[test]
fn test_cancel_success() {
    let (mut harness, fixture, plan) = setup_fixture();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    let meta = harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber_pubkey,
            plan.plan_id,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect("cancel should succeed");

    assert!(meta.compute_units_consumed > 0);

    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert!(matches!(mandate.status, MandateStatus::Cancelled));

    let credential_ata = harness.derive_credential_ata(
        &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
        &fixture.credential_mint,
    );
    let credential_account =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential account should unpack");
    assert_eq!(credential_account.amount, 0);

    let subscriber_token = harness.fetch_spl_token_account(&fixture.subscriber_token_account);
    assert!(subscriber_token.delegate.is_none());
}

#[test]
fn test_cancel_unauthorized() {
    let (mut harness, fixture, plan) = setup_fixture();
    let attacker = harness.create_wallet();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    let error = harness
        .send_cancel(
            &attacker,
            &subscriber_pubkey,
            plan.plan_id,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect_err("cancel should reject unauthorized callers");

    assert!(
        format!("{:?}", error.err).contains("Custom(6004)"),
        "expected UnauthorizedCancel custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_cancel_cu_budget() {
    let (mut harness, fixture, plan) = setup_fixture();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    let meta = harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber_pubkey,
            plan.plan_id,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect("cancel should succeed");

    assert!(
        meta.compute_units_consumed < 150_000,
        "cancel CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}
