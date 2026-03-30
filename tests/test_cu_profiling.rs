#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_signer::Signer;
use vela_protocol::constants::MIN_FREQUENCY_SECONDS;

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
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());

    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");
    let usdc_mint = harness.create_spl_mint(&subscriber, 6);
    let subscriber_token_account =
        harness.create_spl_token_account(&subscriber, &usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(
        &subscriber,
        &usdc_mint,
        &subscriber_token_account,
        25_000_000 * 5,
    );

    let meta = harness
        .send_subscribe(&subscriber, 0, &subscriber_token_account, &usdc_mint)
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
    let mandate: vela_protocol::state::VelaMandate = harness.fetch_anchor_account(&fixture.mandate);

    harness.set_clock_timestamp(mandate.next_payment_due);
    let meta = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            0,
            &fixture.subscriber_token_account,
            &fixture.merchant_token_account,
            &fixture.usdc_mint,
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
    let usdc_mint = harness.create_spl_mint(&subscriber, 6);
    let subscriber_token_account =
        harness.create_spl_token_account(&subscriber, &usdc_mint, &subscriber_pubkey);
    let merchant_token_account =
        harness.create_spl_token_account(&subscriber, &usdc_mint, &harness.merchant_pubkey());
    harness.mint_spl_tokens(
        &subscriber,
        &usdc_mint,
        &subscriber_token_account,
        25_000_000 * 5,
    );

    let subscribe_meta = harness
        .send_subscribe(&subscriber, 0, &subscriber_token_account, &usdc_mint)
        .expect("subscribe should succeed");
    let addresses = harness.derive_plan_addresses(0);
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &addresses.plan);
    let mandate_account: vela_protocol::state::VelaMandate = harness.fetch_anchor_account(&mandate);

    harness.set_clock_timestamp(mandate_account.next_payment_due);
    let execute_meta = harness
        .send_execute_pull(
            &subscriber,
            &subscriber_pubkey,
            0,
            &subscriber_token_account,
            &merchant_token_account,
            &usdc_mint,
        )
        .expect("execute_pull should succeed");
    let cancel_meta = harness
        .send_cancel(
            &subscriber,
            &subscriber_pubkey,
            0,
            &mandate,
            &subscriber_token_account,
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
