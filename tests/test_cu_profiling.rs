#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::constants::MIN_FREQUENCY_SECONDS;
use vela_protocol::state::VelaMandate;

fn setup_wrapped_pull_accounts(
    harness: &mut TestHarness,
    subscriber_signer: &Keypair,
    subscriber_pubkey: &Pubkey,
    mandate: &Pubkey,
    amount: u64,
) -> (Pubkey, Pubkey, Pubkey) {
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);

    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);

    let subscriber_usdc =
        harness.create_spl_token_account(subscriber_signer, &spl_usdc_mint, subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, amount * 4);

    let subscriber_wrapped_pubkey =
        harness.create_token_2022_ata(&admin, mandate, &wrapped_mint_pubkey);
    harness
        .send_wrap(
            subscriber_signer,
            &spl_usdc_mint,
            &wrapped_mint_pubkey,
            &subscriber_usdc,
            &subscriber_wrapped_pubkey,
            mandate,
            &wrapping_vault,
            amount * 2,
        )
        .expect("wrap into mandate account should succeed");

    let merchant_wrapped_pubkey =
        harness.create_token_2022_ata(&admin, &harness.merchant_pubkey(), &wrapped_mint_pubkey);

    (
        subscriber_wrapped_pubkey,
        merchant_wrapped_pubkey,
        wrapped_mint_pubkey,
    )
}

#[test]
fn test_cu_create_plan() {
    let mut harness = TestHarness::new();
    let meta = harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");

    println!("create_plan CU: {}", meta.compute_units_consumed);
    assert!(
        meta.compute_units_consumed < 150_000,
        "create_plan CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}

#[test]
fn test_cu_subscribe() {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();

    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");

    let meta = harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    println!("subscribe CU: {}", meta.compute_units_consumed);
    assert!(
        meta.compute_units_consumed < 150_000,
        "subscribe CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}

#[test]
fn test_cu_execute_pull() {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    let (subscriber_wrapped_pubkey, merchant_wrapped_pubkey, wrapped_mint_pubkey) =
        setup_wrapped_pull_accounts(
            &mut harness,
            &fixture.subscriber,
            &subscriber,
            &fixture.mandate,
            25_000_000,
        );

    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.set_clock_timestamp(mandate.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        25_000_000,
    );

    let meta = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            0,
            &subscriber_wrapped_pubkey,
            &merchant_wrapped_pubkey,
            &wrapped_mint_pubkey,
        )
        .expect("execute_pull should succeed");

    println!("execute_pull CU: {}", meta.compute_units_consumed);
    assert!(
        meta.compute_units_consumed < 150_000,
        "execute_pull CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}

#[test]
fn test_cu_cancel() {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    let meta = harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber,
            0,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect("cancel should succeed");

    println!("cancel CU: {}", meta.compute_units_consumed);
    assert!(
        meta.compute_units_consumed < 150_000,
        "cancel CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}

#[test]
fn test_cu_full_lifecycle() {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());

    let create_meta = harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");

    let subscribe_meta = harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let addresses = harness.derive_plan_addresses(0);
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &addresses.plan);
    let mandate_account: VelaMandate = harness.fetch_anchor_account(&mandate);

    let (subscriber_wrapped_pubkey, merchant_wrapped_pubkey, wrapped_mint_pubkey) =
        setup_wrapped_pull_accounts(
            &mut harness,
            &subscriber,
            &subscriber_pubkey,
            &mandate,
            25_000_000,
        );

    harness.set_clock_timestamp(mandate_account.next_payment_due);
    harness.create_pull_approval_with_amount(
        &mandate,
        mandate_account.next_payment_due,
        true,
        25_000_000,
    );

    let execute_meta = harness
        .send_execute_pull(
            &subscriber,
            &subscriber_pubkey,
            0,
            &subscriber_wrapped_pubkey,
            &merchant_wrapped_pubkey,
            &wrapped_mint_pubkey,
        )
        .expect("execute_pull should succeed");

    // cancel still expects an SPL Token account with subscriber as owner (for the revoke CPI).
    // Create a dummy SPL USDC account for the subscriber.
    let usdc_mint = harness.create_spl_mint(&subscriber, 6);
    let subscriber_spl_account =
        harness.create_spl_token_account(&subscriber, &usdc_mint, &subscriber_pubkey);

    let cancel_meta = harness
        .send_cancel(
            &subscriber,
            &subscriber_pubkey,
            0,
            &mandate,
            &subscriber_spl_account,
        )
        .expect("cancel should succeed");

    let total_cu = create_meta.compute_units_consumed
        + subscribe_meta.compute_units_consumed
        + execute_meta.compute_units_consumed
        + cancel_meta.compute_units_consumed;
    println!("create_plan CU: {}", create_meta.compute_units_consumed);
    println!("subscribe CU: {}", subscribe_meta.compute_units_consumed);
    println!("execute_pull CU: {}", execute_meta.compute_units_consumed);
    println!("cancel CU: {}", cancel_meta.compute_units_consumed);
    println!("full lifecycle total CU: {total_cu}");

    for (label, cu) in [
        ("create_plan", create_meta.compute_units_consumed),
        ("subscribe", subscribe_meta.compute_units_consumed),
        ("execute_pull", execute_meta.compute_units_consumed),
        ("cancel", cancel_meta.compute_units_consumed),
    ] {
        assert!(cu < 150_000, "{label} CU budget exceeded: {cu}");
    }
}
