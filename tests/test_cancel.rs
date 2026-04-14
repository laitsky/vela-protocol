#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{SubscriptionFixture, TestHarness};
use solana_account::Account;
use solana_address::Address;
use solana_program_pack::Pack;
use solana_signer::Signer;
use spl_token_2022::state::Account as Token2022Account;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{BillingType, MandateStatus, VelaMandate, VelaPlan},
};

fn setup_fixture() -> (TestHarness, SubscriptionFixture, VelaPlan) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    (harness, fixture, plan)
}

fn build_v1_mandate_bytes(
    subscriber: &anchor_lang::prelude::Pubkey,
    plan: &anchor_lang::prelude::Pubkey,
    merchant: &anchor_lang::prelude::Pubkey,
    amount: u64,
    frequency: u64,
    start_date: i64,
    expiry: i64,
    max_pulls: u64,
    next_payment_due: i64,
    bump: u8,
) -> Vec<u8> {
    let mut data = Vec::new();
    anchor_lang::AnchorSerialize::serialize(subscriber, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(plan, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(merchant, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&amount, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&frequency, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&start_date, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&expiry, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&max_pulls, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&0u64, &mut data).unwrap(); // pulls_executed
    anchor_lang::AnchorSerialize::serialize(&next_payment_due, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&0i64, &mut data).unwrap(); // last_pull_at
    anchor_lang::AnchorSerialize::serialize(&0u64, &mut data).unwrap(); // last_billing_recorded_pull
    anchor_lang::AnchorSerialize::serialize(&0u64, &mut data).unwrap(); // validation_request_nonce
    anchor_lang::AnchorSerialize::serialize(&0u64, &mut data).unwrap(); // billing_request_nonce
    anchor_lang::AnchorSerialize::serialize(&MandateStatus::Active, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&bump, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&BillingType::Flat, &mut data).unwrap();
    data
}

fn inject_legacy_mandate(
    harness: &mut TestHarness,
    subscriber: &anchor_lang::prelude::Pubkey,
    plan: &anchor_lang::prelude::Pubkey,
    amount: u64,
    frequency: u64,
    max_pulls: u64,
) -> anchor_lang::prelude::Pubkey {
    use anchor_lang::Discriminator;
    let merchant = harness.merchant_pubkey();
    let (legacy_mandate, bump) = anchor_lang::prelude::Pubkey::find_program_address(
        &[
            vela_protocol::state::VelaMandate::SEED_PREFIX,
            subscriber.as_ref(),
            plan.as_ref(),
        ],
        &vela_protocol::ID,
    );
    let now = harness.current_timestamp();
    let body = build_v1_mandate_bytes(
        subscriber,
        plan,
        &merchant,
        amount,
        frequency,
        now,
        now + (frequency * max_pulls) as i64,
        max_pulls,
        now + frequency as i64,
        bump,
    );
    let mut full_data = Vec::new();
    full_data.extend_from_slice(&VelaMandate::DISCRIMINATOR);
    full_data.extend_from_slice(&body);
    harness
        .svm
        .set_account(
            Address::from(legacy_mandate.to_bytes()),
            Account {
                lamports: 2_000_000,
                data: full_data,
                owner: harness.program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("legacy mandate should be injected");
    legacy_mandate
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

#[test]
fn test_cancel_and_admin_cancel_support_legacy_and_v2_mandates() {
    let (mut harness, fixture, plan) = setup_fixture();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    // V2 path with admin_cancel.
    harness
        .send_admin_cancel(
            &admin,
            &subscriber_pubkey,
            &fixture.plan,
            &fixture.mandate,
            &fixture.credential_mint,
        )
        .expect("admin_cancel should support V2 mandates");
    let v2_mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert!(matches!(v2_mandate.status, MandateStatus::Cancelled));

    // legacy path with subscriber cancel.
    let legacy_mandate = inject_legacy_mandate(
        &mut harness,
        &subscriber_pubkey,
        &fixture.plan,
        plan.amount,
        plan.frequency,
        plan.max_pulls,
    );
    harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber_pubkey,
            plan.plan_id,
            &legacy_mandate,
            &fixture.subscriber_token_account,
        )
        .expect("cancel should support legacy mandates");
}

#[test]
fn test_cancel_merchant_first_and_fallback_credential_resolution() {
    let (mut harness, fixture, plan) = setup_fixture();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    // merchant-first resolution for V2 mandates.
    harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber_pubkey,
            plan.plan_id,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect("cancel should burn merchant credential first");

    // fallback resolution for legacy mandates.
    let legacy_mandate = inject_legacy_mandate(
        &mut harness,
        &subscriber_pubkey,
        &fixture.plan,
        plan.amount,
        plan.frequency,
        plan.max_pulls,
    );
    harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber_pubkey,
            plan.plan_id,
            &legacy_mandate,
            &fixture.subscriber_token_account,
        )
        .expect("cancel should fallback to plan credential for legacy namespace");
}
