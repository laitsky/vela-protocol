#[path = "helpers/mod.rs"]
mod helpers;

use std::any::type_name;

use anchor_lang::__private::base64::{engine::general_purpose::STANDARD, Engine as _};
use anchor_lang::{AnchorDeserialize, Discriminator};
use helpers::TestHarness;
use litesvm::types::TransactionMetadata;
use solana_signer::Signer;
use vela_protocol::constants::WRAPPED_USDC_SYMBOL;
use vela_protocol::state::{
    MandateUpgradeFinalized, MandateUpgradeInitiated, MerchantState, StreamCancelled,
    StreamCreated, StreamMandate, StreamPaused, StreamRateUpdated, StreamResumed, StreamSettled,
};

struct StreamFixture {
    harness: TestHarness,
    subscriber: solana_keypair::Keypair,
    stream_mandate: anchor_lang::prelude::Pubkey,
    subscriber_wrapped: anchor_lang::prelude::Pubkey,
    merchant_wrapped: anchor_lang::prelude::Pubkey,
    wrapped_mint: anchor_lang::prelude::Pubkey,
    created_at: i64,
}

fn setup_stream_fixture(
    rate_per_second: u64,
    authorized_max_rate: u64,
    max_streamed: Option<u64>,
    min_settle_interval: u32,
    wrapped_amount: u64,
) -> StreamFixture {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);

    let wrapped_mint = solana_keypair::Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);
    harness
        .send_init_merchant_credential()
        .expect("merchant credential bootstrap should succeed");

    let merchant = harness.merchant_pubkey();
    let (merchant_state, _) = anchor_lang::prelude::Pubkey::find_program_address(
        &[MerchantState::SEED_PREFIX, merchant.as_ref()],
        &vela_protocol::ID,
    );
    let merchant_state_before: MerchantState = harness.fetch_anchor_account(&merchant_state);

    harness
        .send_create_stream_mandate(
            &subscriber,
            &wrapped_mint_pubkey,
            rate_per_second,
            authorized_max_rate,
            max_streamed,
            min_settle_interval,
        )
        .expect("create_stream_mandate should succeed");

    let stream_mandate = harness.derive_stream_mandate_address_by_index(
        &subscriber_pubkey,
        &merchant,
        merchant_state_before.stream_mandate_counter,
    );
    let created_stream: StreamMandate = harness.fetch_anchor_account(&stream_mandate);
    harness.create_pull_approval_with_amount(
        &stream_mandate,
        created_stream.last_settled_ts + 600,
        true,
        u64::MAX,
    );

    let subscriber_usdc =
        harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, wrapped_amount);
    let subscriber_wrapped =
        harness.create_token_2022_ata(&admin, &stream_mandate, &wrapped_mint_pubkey);
    harness
        .send_wrap(
            &subscriber,
            &spl_usdc_mint,
            &wrapped_mint_pubkey,
            &subscriber_usdc,
            &subscriber_wrapped,
            &stream_mandate,
            &wrapping_vault,
            wrapped_amount,
        )
        .expect("wrap into stream mandate account should succeed");
    let merchant_wrapped = harness.create_token_2022_ata(&admin, &merchant, &wrapped_mint_pubkey);

    StreamFixture {
        harness,
        subscriber,
        stream_mandate,
        subscriber_wrapped,
        merchant_wrapped,
        wrapped_mint: wrapped_mint_pubkey,
        created_at: created_stream.last_settled_ts,
    }
}

fn decode_event<T>(metadata: &TransactionMetadata) -> Vec<T>
where
    T: AnchorDeserialize + Discriminator,
{
    metadata
        .logs
        .iter()
        .filter_map(|log| log.strip_prefix("Program data: "))
        .filter_map(|encoded| {
            let raw = STANDARD.decode(encoded).ok()?;
            if !raw.starts_with(T::DISCRIMINATOR) {
                return None;
            }
            let mut slice: &[u8] = &raw[T::DISCRIMINATOR.len()..];
            T::deserialize(&mut slice).ok()
        })
        .collect()
}

fn assert_single_event<T>(metadata: &TransactionMetadata) -> T
where
    T: AnchorDeserialize + Discriminator,
{
    let events = decode_event::<T>(metadata);
    assert_eq!(
        events.len(),
        1,
        "expected exactly one {} event, logs={:?}",
        type_name::<T>(),
        metadata.logs,
    );
    events.into_iter().next().expect("single event")
}

#[test]
fn test_create_stream_mandate_emits_stream_created() {
    let mut fixture = setup_stream_fixture(10, 15, Some(5_000), 60, 2_000);
    let merchant = fixture.harness.merchant_pubkey();
    let event = assert_single_event::<StreamCreated>(
        &fixture
            .harness
            .send_create_stream_mandate(
                &fixture.subscriber,
                &fixture.wrapped_mint,
                20,
                25,
                Some(10_000),
                60,
            )
            .expect("second stream creation should succeed"),
    );

    assert_eq!(event.schema_version, 1);
    assert_eq!(
        event.subscriber,
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes())
    );
    assert_eq!(event.merchant, merchant);
    assert_eq!(event.mint, fixture.wrapped_mint);
    assert_eq!(event.token_symbol, "");
    assert_eq!(event.rate_per_second, 20);
    assert_eq!(event.authorized_max_rate, 25);
    assert_eq!(event.max_streamed, Some(10_000));
    assert_eq!(event.min_settle_interval, 60);
}

#[test]
fn test_execute_stream_emits_stream_settled() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 120);
    let metadata = fixture
        .harness
        .send_execute_stream(
            &fixture.harness.merchant.insecure_clone(),
            &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("execute_stream should succeed");

    let event = assert_single_event::<StreamSettled>(&metadata);
    assert_eq!(event.schema_version, 1);
    assert_eq!(event.mandate, fixture.stream_mandate);
    assert_eq!(event.mint, fixture.wrapped_mint);
    assert_eq!(event.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_eq!(event.amount, 1_200);
    assert_eq!(event.total_streamed_after, 1_200);
    assert_eq!(event.last_settled_ts, fixture.created_at + 120);
    assert_eq!(event.timestamp, fixture.created_at + 120);
}

#[test]
fn test_pause_stream_emits_stream_paused() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 120);
    let metadata = fixture
        .harness
        .send_pause_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("pause_stream should succeed");

    let event = assert_single_event::<StreamPaused>(&metadata);
    assert_eq!(event.schema_version, 1);
    assert_eq!(event.mandate, fixture.stream_mandate);
    assert_eq!(event.mint, fixture.wrapped_mint);
    assert_eq!(event.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_eq!(event.paused_at, fixture.created_at + 120);
    assert_eq!(
        event.signer,
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes())
    );
    assert_eq!(event.final_settle_amount, 1_200);
    assert_eq!(event.timestamp, fixture.created_at + 120);
}

#[test]
fn test_resume_stream_emits_stream_resumed() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 120);
    fixture
        .harness
        .send_pause_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("pause_stream should succeed");
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 420);

    let metadata = fixture
        .harness
        .send_resume_stream(&fixture.subscriber, &fixture.stream_mandate)
        .expect("resume_stream should succeed");

    let event = assert_single_event::<StreamResumed>(&metadata);
    assert_eq!(event.schema_version, 1);
    assert_eq!(event.mandate, fixture.stream_mandate);
    assert_eq!(event.mint, fixture.wrapped_mint);
    assert_eq!(event.token_symbol, "");
    assert_eq!(event.resumed_at, fixture.created_at + 420);
    assert_eq!(event.pause_duration_secs, 300);
    assert_eq!(
        event.signer,
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes())
    );
    assert_eq!(event.timestamp, fixture.created_at + 420);
}

#[test]
fn test_update_stream_rate_emits_stream_rate_updated() {
    let mut fixture = setup_stream_fixture(10, 20, None, 60, 10_000);
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 120);
    let metadata = fixture
        .harness
        .send_update_stream_rate(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
            Some(15),
            Some(25),
        )
        .expect("update_stream_rate should succeed");

    let event = assert_single_event::<StreamRateUpdated>(&metadata);
    assert_eq!(event.schema_version, 1);
    assert_eq!(event.mandate, fixture.stream_mandate);
    assert_eq!(event.mint, fixture.wrapped_mint);
    assert_eq!(event.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_eq!(event.old_rate_per_second, 10);
    assert_eq!(event.new_rate_per_second, 15);
    assert_eq!(event.old_authorized_max_rate, 20);
    assert_eq!(event.new_authorized_max_rate, 25);
    assert_eq!(
        event.signer,
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes())
    );
    assert_eq!(event.final_settle_amount, 1_200);
    assert_eq!(event.timestamp, fixture.created_at + 120);

    let initiated = assert_single_event::<MandateUpgradeInitiated>(&metadata);
    assert_eq!(initiated.mandate, fixture.stream_mandate);
    assert_eq!(initiated.old_plan, anchor_lang::prelude::Pubkey::default());
    assert_eq!(initiated.new_plan, anchor_lang::prelude::Pubkey::default());
    assert_eq!(initiated.proration_amount, 1_200);
    assert_eq!(initiated.change_type, 1);
    assert_eq!(initiated.mint, fixture.wrapped_mint);
    assert_eq!(initiated.token_symbol, WRAPPED_USDC_SYMBOL);

    let finalized = assert_single_event::<MandateUpgradeFinalized>(&metadata);
    assert_eq!(finalized.mandate, fixture.stream_mandate);
    assert_eq!(finalized.old_plan, anchor_lang::prelude::Pubkey::default());
    assert_eq!(finalized.new_plan, anchor_lang::prelude::Pubkey::default());
    assert_eq!(finalized.proration_amount, 1_200);
    assert_eq!(finalized.change_type, 1);
    assert_eq!(finalized.mint, fixture.wrapped_mint);
    assert_eq!(finalized.token_symbol, WRAPPED_USDC_SYMBOL);
}

#[test]
fn test_cancel_stream_emits_stream_cancelled() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 120);
    let metadata = fixture
        .harness
        .send_cancel_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("cancel_stream should succeed");

    let event = assert_single_event::<StreamCancelled>(&metadata);
    assert_eq!(event.schema_version, 1);
    assert_eq!(event.mandate, fixture.stream_mandate);
    assert_eq!(event.mint, fixture.wrapped_mint);
    assert_eq!(event.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_eq!(event.cancelled_at, fixture.created_at + 120);
    assert_eq!(
        event.signer,
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes())
    );
    assert_eq!(event.final_settle_amount, 1_200);
    assert_eq!(event.total_streamed_final, 1_200);
    assert_eq!(event.timestamp, fixture.created_at + 120);
}

#[test]
fn test_stream_event_schema_contract_is_documented() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should resolve");
    let source = std::fs::read_to_string(
        repo_root.join("programs/vela-protocol/src/state/billing_event.rs"),
    )
    .expect("billing_event source should exist");

    assert!(
        source.contains("additive-only"),
        "billing_event.rs must document the additive-only stream event contract",
    );
}
