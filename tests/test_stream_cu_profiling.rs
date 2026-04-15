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
fn test_cu_execute_stream_vs_execute_pull() {
    let mut pull_harness = TestHarness::new();
    let fixture = pull_harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    let (subscriber_wrapped_pubkey, merchant_wrapped_pubkey, wrapped_mint_pubkey) =
        setup_wrapped_pull_accounts(
            &mut pull_harness,
            &fixture.subscriber,
            &subscriber,
            &fixture.mandate,
            25_000_000,
        );

    let mandate: VelaMandate = pull_harness.fetch_anchor_account(&fixture.mandate);
    pull_harness.set_clock_timestamp(mandate.next_payment_due);
    pull_harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        25_000_000,
    );

    let pull_meta = pull_harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            0,
            &subscriber_wrapped_pubkey,
            &merchant_wrapped_pubkey,
            &wrapped_mint_pubkey,
        )
        .expect("execute_pull should succeed");
    let pull_cu = pull_meta.compute_units_consumed;

    let mut stream_harness = TestHarness::new();
    stream_harness
        .send_create_stream_mandate_proto(1_000_000, 2_000_000, Some(150_000_000), 60)
        .expect("create_stream_mandate_proto should succeed");
    let stream_mandate = stream_harness.last_stream_mandate_proto();
    let stream_state = stream_harness.fetch_account_data(&stream_mandate);
    assert!(
        !stream_state.is_empty(),
        "stream mandate proto account should be created"
    );
    stream_harness.set_clock_timestamp(stream_harness.current_timestamp() + 90);
    let stream_meta = stream_harness
        .send_execute_stream_proto(stream_mandate)
        .expect("execute_stream_proto should succeed");
    let stream_cu = stream_meta.compute_units_consumed;

    println!("execute_pull CU: {pull_cu}");
    println!("execute_stream CU: {stream_cu}");
    println!("CU delta: {}", stream_cu as i64 - pull_cu as i64);

    assert!(
        stream_cu < 400_000,
        "execute_stream CU budget exceeded: {stream_cu}",
    );
}
