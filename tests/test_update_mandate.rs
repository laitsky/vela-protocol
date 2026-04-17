#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_address::Address;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{BillingType, VelaMandate},
};

fn setup_update_fixture() -> (TestHarness, solana_keypair::Keypair, Pubkey, Pubkey, Pubkey) {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create plan 0 should succeed");
    harness
        .send_create_plan(45_000_000, MIN_FREQUENCY_SECONDS * 2, 0, 6, 1)
        .expect("create plan 1 should succeed");
    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let plan_0 = harness.derive_plan_addresses(0).plan;
    let plan_1 = harness.derive_plan_addresses(1).plan;
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &plan_0);

    (harness, subscriber, mandate, plan_0, plan_1)
}

#[test]
fn test_update_mandate_merchant_only_and_plan_switch_keeps_address() {
    let (mut harness, _subscriber, mandate, _plan_0, plan_1) = setup_update_fixture();
    let updated_amount = 33_000_000;
    let updated_frequency = MIN_FREQUENCY_SECONDS * 3;
    let updated_max_pulls = 9;

    harness
        .send_update_mandate(
            &mandate,
            Some(plan_1),
            Some(updated_amount),
            Some(updated_frequency),
            Some(updated_max_pulls),
            Some(BillingType::Flat),
        )
        .expect("update_mandate should succeed for merchant");

    let mandate_after: VelaMandate = harness.fetch_anchor_account(&mandate);
    assert_eq!(mandate_after.plan, plan_1);
    assert_eq!(mandate_after.amount, updated_amount);
    assert_eq!(mandate_after.frequency, updated_frequency);
    assert_eq!(mandate_after.max_pulls, updated_max_pulls);
}

#[test]
fn test_update_mandate_rejects_non_merchant_signer() {
    let (mut harness, _subscriber, mandate, _plan_0, plan_1) = setup_update_fixture();
    let attacker = harness.create_wallet();
    let before: VelaMandate = harness.fetch_anchor_account(&mandate);

    let error = harness
        .send_update_mandate_as(
            &attacker,
            &mandate,
            Some(plan_1),
            Some(before.amount + 1),
            None,
            None,
            None,
        )
        .expect_err("non-merchant signer must be rejected");

    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom error for merchant-only update, got {:?}",
        error.err
    );

    let after: VelaMandate = harness.fetch_anchor_account(&mandate);
    assert_eq!(after.plan, before.plan);
    assert_eq!(after.amount, before.amount);
}

#[test]
fn test_update_mandate_realloc_tail_compatibility() {
    // realloc: mandate account should remain updatable at the same address after trailing-byte growth.
    let (mut harness, _subscriber, mandate, _plan_0, _plan_1) = setup_update_fixture();
    let before: VelaMandate = harness.fetch_anchor_account(&mandate);
    let mut mandate_account = harness
        .svm
        .get_account(&Address::from(mandate.to_bytes()))
        .expect("mandate account should exist");
    let original_len = mandate_account.data.len();
    mandate_account.data.extend_from_slice(&[0u8; 24]);
    let grown_len = mandate_account.data.len();
    harness
        .svm
        .set_account(Address::from(mandate.to_bytes()), mandate_account)
        .expect("mandate account should be reinserted");
    assert!(grown_len > original_len);

    harness
        .send_update_mandate(&mandate, None, Some(before.amount + 7), None, None, None)
        .expect("update_mandate should succeed after tail realloc growth");

    let after: VelaMandate = harness.fetch_anchor_account(&mandate);
    assert_eq!(after.amount, before.amount + 7);
}
