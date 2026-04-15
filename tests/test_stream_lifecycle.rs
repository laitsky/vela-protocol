#[path = "helpers/mod.rs"]
mod helpers;

use helpers::TestHarness;
use solana_keypair::Keypair;
use vela_protocol::state::{MerchantState, StreamMandate, StreamStatus};

#[test]
fn test_create_stream_mandate() {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);

    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, _wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness
        .send_init_merchant_credential()
        .expect("merchant state bootstrap should succeed");

    let merchant_state = harness
        .derive_plan_addresses(0)
        .merchant_state;
    let merchant_state_before: MerchantState = harness.fetch_anchor_account(&merchant_state);

    harness
        .send_create_stream_mandate(&subscriber, &wrapped_mint_pubkey, 10, 10, Some(5_000), 60)
        .expect("create_stream_mandate should succeed");

    let merchant_state_after: MerchantState = harness.fetch_anchor_account(&merchant_state);
    let mandate = harness.derive_stream_mandate_address_by_index(
        &subscriber.pubkey().into(),
        &harness.merchant_pubkey(),
        merchant_state_before.stream_mandate_counter,
    );
    let stream: StreamMandate = harness.fetch_anchor_account(&mandate);

    assert_eq!(
        merchant_state_after.stream_mandate_counter,
        merchant_state_before.stream_mandate_counter + 1
    );
    assert_eq!(stream.rate_per_second, 10);
    assert_eq!(stream.authorized_max_rate, 10);
    assert_eq!(stream.max_streamed, Some(5_000));
    assert_eq!(stream.min_settle_interval, 60);
    assert_eq!(stream.total_streamed, 0);
    assert!(matches!(stream.status, StreamStatus::Active));
}
