#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_address::Address;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{MandateStatus, VelaMandate},
};

fn setup_close_fixture() -> (TestHarness, solana_keypair::Keypair, Pubkey) {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create plan should succeed");
    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let plan = harness.derive_plan_addresses(0).plan;
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &plan);

    (harness, subscriber, mandate)
}

#[test]
fn test_close_mandate_only_cancelled_or_expired() {
    let (mut harness, subscriber, mandate) = setup_close_fixture();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());

    let active_error = harness
        .send_close_mandate(&subscriber, &subscriber_pubkey, &mandate)
        .expect_err("active mandates must not be closable");
    assert!(
        format!("{:?}", active_error.err).contains("Custom("),
        "expected custom error for active close, got {:?}",
        active_error.err
    );

    let mut cancelled: VelaMandate = harness.fetch_anchor_account(&mandate);
    cancelled.status = MandateStatus::Cancelled;
    harness.overwrite_anchor_account(&mandate, &cancelled);

    let before_subscriber_lamports = harness
        .svm
        .get_account(&Address::from(subscriber_pubkey.to_bytes()))
        .expect("subscriber account should exist")
        .lamports;
    harness
        .send_close_mandate(&subscriber, &subscriber_pubkey, &mandate)
        .expect("cancelled mandate should close");
    assert!(
        harness
            .svm
            .get_account(&Address::from(mandate.to_bytes()))
            .is_none(),
        "mandate account should be closed"
    );
    let after_subscriber_lamports = harness
        .svm
        .get_account(&Address::from(subscriber_pubkey.to_bytes()))
        .expect("subscriber account should still exist")
        .lamports;
    assert!(after_subscriber_lamports > before_subscriber_lamports);
}

#[test]
fn test_close_mandate_allows_expired_status() {
    let (mut harness, subscriber, mandate) = setup_close_fixture();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let mut expired: VelaMandate = harness.fetch_anchor_account(&mandate);
    expired.status = MandateStatus::Expired;
    harness.overwrite_anchor_account(&mandate, &expired);

    harness
        .send_close_mandate(&subscriber, &subscriber_pubkey, &mandate)
        .expect("expired mandate should close");
    assert!(
        harness
            .svm
            .get_account(&Address::from(mandate.to_bytes()))
            .is_none(),
        "mandate account should be closed"
    );
}

#[test]
fn test_close_mandate_rejects_non_subscriber_authority() {
    let (mut harness, subscriber, mandate) = setup_close_fixture();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let mut cancelled: VelaMandate = harness.fetch_anchor_account(&mandate);
    cancelled.status = MandateStatus::Cancelled;
    harness.overwrite_anchor_account(&mandate, &cancelled);

    let error = harness
        .send_close_mandate(&harness.merchant.insecure_clone(), &subscriber_pubkey, &mandate)
        .expect_err("non-subscriber authority must be rejected");
    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom error for unauthorized close, got {:?}",
        error.err
    );
}
