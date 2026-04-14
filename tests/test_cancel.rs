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
    let (mut harness_v2, fixture_v2, _plan_v2) = setup_fixture();
    let subscriber_v2 =
        anchor_lang::prelude::Pubkey::new_from_array(fixture_v2.subscriber.pubkey().to_bytes());
    let admin_v2 = harness_v2.merchant.insecure_clone();
    harness_v2.init_protocol_config(&admin_v2);

    // V2 path with admin_cancel.
    harness_v2
        .send_admin_cancel(
            &admin_v2,
            &subscriber_v2,
            &fixture_v2.plan,
            &fixture_v2.mandate,
            &fixture_v2.credential_mint,
        )
        .expect("admin_cancel should support V2 mandates");
    let v2_mandate: VelaMandate = harness_v2.fetch_anchor_account(&fixture_v2.mandate);
    assert!(matches!(v2_mandate.status, MandateStatus::Cancelled));

    // legacy path with subscriber cancel.
    let (mut harness_legacy, fixture_legacy, plan_legacy) = setup_fixture();
    let subscriber_legacy =
        anchor_lang::prelude::Pubkey::new_from_array(fixture_legacy.subscriber.pubkey().to_bytes());
    let legacy_mandate = inject_legacy_mandate(
        &mut harness_legacy,
        &subscriber_legacy,
        &fixture_legacy.plan,
        plan_legacy.amount,
        plan_legacy.frequency,
        plan_legacy.max_pulls,
    );
    harness_legacy
        .send_cancel(
            &fixture_legacy.subscriber,
            &subscriber_legacy,
            plan_legacy.plan_id,
            &legacy_mandate,
            &fixture_legacy.subscriber_token_account,
        )
        .expect("cancel should support legacy mandates");
}

#[test]
fn test_cancel_merchant_first_and_fallback_credential_resolution() {
    let (mut harness_merchant, fixture_merchant, plan_merchant) = setup_fixture();
    let subscriber_merchant =
        anchor_lang::prelude::Pubkey::new_from_array(fixture_merchant.subscriber.pubkey().to_bytes());

    // merchant-first resolution for V2 mandates.
    harness_merchant
        .send_cancel(
            &fixture_merchant.subscriber,
            &subscriber_merchant,
            plan_merchant.plan_id,
            &fixture_merchant.mandate,
            &fixture_merchant.subscriber_token_account,
        )
        .expect("cancel should burn merchant credential first");

    // fallback resolution for legacy mandates.
    let (mut harness_fallback, fixture_fallback, mut plan_fallback) = setup_fixture();
    let subscriber_fallback =
        anchor_lang::prelude::Pubkey::new_from_array(fixture_fallback.subscriber.pubkey().to_bytes());
    let fallback_mint = anchor_lang::prelude::Pubkey::new_unique();
    harness_fallback.inject_token_2022_mint(&fallback_mint, &fixture_fallback.plan, 1);
    let fallback_ata = harness_fallback.derive_credential_ata(&subscriber_fallback, &fallback_mint);
    harness_fallback.inject_token_2022_account(
        &fallback_ata,
        &fallback_mint,
        &subscriber_fallback,
        1,
    );

    let mut merchant_state: vela_protocol::state::MerchantState =
        harness_fallback.fetch_anchor_account(&harness_fallback.derive_plan_addresses(plan_fallback.plan_id).merchant_state);
    merchant_state.credential_mint = anchor_lang::prelude::Pubkey::default();
    harness_fallback.overwrite_anchor_account(
        &harness_fallback.derive_plan_addresses(plan_fallback.plan_id).merchant_state,
        &merchant_state,
    );
    plan_fallback.credential_mint = fallback_mint;
    harness_fallback.overwrite_anchor_account(&fixture_fallback.plan, &plan_fallback);

    let legacy_mandate = inject_legacy_mandate(
        &mut harness_fallback,
        &subscriber_fallback,
        &fixture_fallback.plan,
        plan_fallback.amount,
        plan_fallback.frequency,
        plan_fallback.max_pulls,
    );
    harness_fallback
        .send_cancel(
            &fixture_fallback.subscriber,
            &subscriber_fallback,
            plan_fallback.plan_id,
            &legacy_mandate,
            &fixture_fallback.subscriber_token_account,
        )
        .expect("cancel should fallback to plan credential for legacy namespace");
}

/// Read legacy mandate status from raw account data.
/// Legacy V1 layout: 8 (discriminator) + 96 (3 Pubkeys) + 88 (11 u64/i64 fields) = 192.
fn read_legacy_mandate_status_byte(harness: &TestHarness, key: &anchor_lang::prelude::Pubkey) -> u8 {
    let data = harness.fetch_account_data(key);
    data[192]
}

#[test]
fn test_admin_cancel_legacy_mandate() {
    let (mut harness, fixture, plan) = setup_fixture();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    let legacy_mandate = inject_legacy_mandate(
        &mut harness,
        &subscriber_pubkey,
        &fixture.plan,
        plan.amount,
        plan.frequency,
        plan.max_pulls,
    );
    harness
        .send_admin_cancel(
            &admin,
            &subscriber_pubkey,
            &fixture.plan,
            &legacy_mandate,
            &fixture.credential_mint,
        )
        .expect("admin_cancel should support legacy mandates");

    // MandateStatus::Cancelled = 1 in the V1 legacy layout
    let status_byte = read_legacy_mandate_status_byte(&harness, &legacy_mandate);
    assert_eq!(
        status_byte, 1,
        "legacy mandate status should be Cancelled (1) after admin_cancel, got {}",
        status_byte
    );
}

#[test]
fn test_admin_cancel_merchant_first_and_fallback_credential_resolution() {
    // merchant-first resolution: admin_cancel with MerchantState.credential_mint set (V2 mandate)
    let (mut harness_merchant, fixture_merchant, _plan_merchant) = setup_fixture();
    let subscriber_merchant = anchor_lang::prelude::Pubkey::new_from_array(
        fixture_merchant.subscriber.pubkey().to_bytes(),
    );
    let admin_merchant = harness_merchant.merchant.insecure_clone();
    harness_merchant.init_protocol_config(&admin_merchant);
    harness_merchant
        .send_admin_cancel(
            &admin_merchant,
            &subscriber_merchant,
            &fixture_merchant.plan,
            &fixture_merchant.mandate,
            &fixture_merchant.credential_mint,
        )
        .expect("admin_cancel should burn via merchant-first credential resolution");

    let v2_mandate: VelaMandate =
        harness_merchant.fetch_anchor_account(&fixture_merchant.mandate);
    assert!(matches!(v2_mandate.status, MandateStatus::Cancelled));

    // plan-fallback resolution: admin_cancel when MerchantState.credential_mint is zeroed.
    // The plan still holds its PDA-derived credential_mint, so the resolver falls back to it
    // and admin_cancel uses the plan PDA as burn signer.
    let (mut harness_fallback, fixture_fallback, plan_fallback) = setup_fixture();
    let subscriber_fallback = anchor_lang::prelude::Pubkey::new_from_array(
        fixture_fallback.subscriber.pubkey().to_bytes(),
    );
    let admin_fallback = harness_fallback.merchant.insecure_clone();
    harness_fallback.init_protocol_config(&admin_fallback);

    // Zero out the merchant credential so resolver falls back to plan.credential_mint
    let merchant_state_addr = harness_fallback
        .derive_plan_addresses(plan_fallback.plan_id)
        .merchant_state;
    let mut merchant_state: vela_protocol::state::MerchantState =
        harness_fallback.fetch_anchor_account(&merchant_state_addr);
    merchant_state.credential_mint = anchor_lang::prelude::Pubkey::default();
    harness_fallback.overwrite_anchor_account(&merchant_state_addr, &merchant_state);

    // Inject a legacy mandate pointing to the plan (subscriber already has a credential
    // token from subscribe_fixture, minted against the plan's PDA-derived credential_mint).
    let legacy_mandate = inject_legacy_mandate(
        &mut harness_fallback,
        &subscriber_fallback,
        &fixture_fallback.plan,
        plan_fallback.amount,
        plan_fallback.frequency,
        plan_fallback.max_pulls,
    );

    harness_fallback
        .send_admin_cancel(
            &admin_fallback,
            &subscriber_fallback,
            &fixture_fallback.plan,
            &legacy_mandate,
            &plan_fallback.credential_mint,
        )
        .expect("admin_cancel should fallback to plan credential when merchant credential is zeroed");

    let status_byte = read_legacy_mandate_status_byte(&harness_fallback, &legacy_mandate);
    assert_eq!(
        status_byte, 1,
        "legacy mandate should be Cancelled (1) via admin_cancel with plan-fallback credential, got {}",
        status_byte
    );
}
