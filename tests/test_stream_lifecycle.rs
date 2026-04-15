#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use litesvm::types::{FailedTransactionMetadata, TransactionMetadata};
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_2022::state::Account as Token2022Account;
use vela_protocol::state::{MerchantState, StreamMandate, StreamStatus, VelaMandate};

struct StreamFixture {
    harness: TestHarness,
    subscriber: Keypair,
    stream_mandate: Pubkey,
    subscriber_wrapped: Pubkey,
    merchant_wrapped: Pubkey,
    wrapped_mint: Pubkey,
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
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);

    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness
        .send_init_merchant_credential()
        .expect("merchant state bootstrap should succeed");

    let merchant = harness.merchant_pubkey();
    let (merchant_state, _) = Pubkey::find_program_address(
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
    harness.create_pull_approval_with_amount(&stream_mandate, created_stream.last_settled_ts, false, 0);

    let subscriber_usdc = harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, wrapped_amount);
    let subscriber_wrapped = harness.create_token_2022_ata(&admin, &stream_mandate, &wrapped_mint_pubkey);
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

fn call_transfer_hook_directly(
    harness: &mut TestHarness,
    source_token: &Pubkey,
    mint: &Pubkey,
    destination_token: &Pubkey,
    owner: &Pubkey,
    wrapping_vault: &Pubkey,
    config: &Pubkey,
    pull_approval: &Pubkey,
    token_config: &Pubkey,
    amount: u64,
    caller: &solana_keypair::Keypair,
) -> Result<TransactionMetadata, FailedTransactionMetadata> {
    let (extra_account_meta_list, _) = harness.derive_extra_account_meta_list(mint);
    let mut data = vec![105, 37, 101, 197, 75, 251, 102, 26];
    data.extend_from_slice(&amount.to_le_bytes());
    let ix = Instruction {
        program_id: harness.hook_program_id,
        accounts: vec![
            AccountMeta::new_readonly(helpers::to_address(*source_token), false),
            AccountMeta::new_readonly(helpers::to_address(*mint), false),
            AccountMeta::new_readonly(helpers::to_address(*destination_token), false),
            AccountMeta::new_readonly(helpers::to_address(*owner), false),
            AccountMeta::new_readonly(helpers::to_address(extra_account_meta_list), false),
            AccountMeta::new_readonly(helpers::to_address(vela_protocol::ID), false),
            AccountMeta::new_readonly(helpers::to_address(*wrapping_vault), false),
            AccountMeta::new_readonly(helpers::to_address(*config), false),
            AccountMeta::new(helpers::to_address(*pull_approval), false),
            AccountMeta::new_readonly(helpers::to_address(*token_config), false),
            AccountMeta::new_readonly(helpers::to_address(anchor_lang::system_program::ID), false),
            AccountMeta::new_readonly(helpers::to_address(anchor_lang::system_program::ID), false),
            AccountMeta::new_readonly(helpers::to_address(anchor_lang::system_program::ID), false),
        ],
        data,
    };
    harness.send_instructions(&[ix], &[caller], Some(&caller.pubkey()))
}

#[test]
fn test_create_stream_mandate() {
    let fixture = setup_stream_fixture(10, 10, Some(5_000), 60, 2_000);
    let merchant = fixture.harness.merchant_pubkey();
    let (merchant_state, _) = Pubkey::find_program_address(
        &[MerchantState::SEED_PREFIX, merchant.as_ref()],
        &vela_protocol::ID,
    );
    let merchant_state_after: MerchantState = fixture.harness.fetch_anchor_account(&merchant_state);
    let stream: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);

    assert_eq!(merchant_state_after.stream_mandate_counter, 1);
    assert_eq!(stream.rate_per_second, 10);
    assert_eq!(stream.authorized_max_rate, 10);
    assert_eq!(stream.max_streamed, Some(5_000));
    assert_eq!(stream.min_settle_interval, 60);
    assert_eq!(stream.total_streamed, 0);
    assert!(matches!(stream.status, StreamStatus::Active));

    let mut invalid_interval = TestHarness::new();
    let subscriber = invalid_interval.create_wallet();
    let admin = invalid_interval.merchant.insecure_clone();
    let spl_usdc_mint = invalid_interval.create_spl_mint(&admin, 6);
    invalid_interval.init_protocol_config(&admin);
    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, _) = invalid_interval.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    invalid_interval
        .send_init_merchant_credential()
        .expect("merchant credential bootstrap should succeed");
    let err = invalid_interval
        .send_create_stream_mandate(&subscriber, &wrapped_mint_pubkey, 10, 10, Some(500), 59)
        .expect_err("min_settle_interval < 60 should fail");
    assert!(format!("{:?}", err.err).contains("Custom(6708)"));

    let err = invalid_interval
        .send_create_stream_mandate(&subscriber, &wrapped_mint_pubkey, 0, 10, Some(500), 60)
        .expect_err("zero rate should fail");
    assert!(format!("{:?}", err.err).contains("Custom(6707)"));

    let err = invalid_interval
        .send_create_stream_mandate(&subscriber, &wrapped_mint_pubkey, 11, 10, Some(500), 60)
        .expect_err("rate above authorized_max_rate should fail");
    assert!(format!("{:?}", err.err).contains("Custom(6709)"));
}

#[test]
fn test_execute_stream_clamped() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 5_000);
    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    fixture
        .harness
        .send_execute_stream(
            &fixture.subscriber,
            &subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("execute_stream should settle 100 seconds of accrual");

    let mandate: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);

    assert_eq!(mandate.total_streamed, 1_000);
    assert_eq!(merchant_wrapped.amount, 1_000);
}

#[test]
fn test_execute_stream_cap_clamp() {
    let mut fixture = setup_stream_fixture(10, 10, Some(500), 60, 5_000);
    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    fixture
        .harness
        .send_execute_stream(
            &fixture.subscriber,
            &subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("execute_stream should clamp to the remaining cap");

    let mandate: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);

    assert_eq!(mandate.total_streamed, 500);
    assert_eq!(merchant_wrapped.amount, 500);
}

#[test]
fn test_hook_misroute_wrong_account_type() {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, vela_protocol::constants::MIN_FREQUENCY_SECONDS, 0, 2);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);
    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) = harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    let config = harness.derive_config();
    let token_config = harness.derive_token_config_address(&wrapped_mint_pubkey);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let subscriber_usdc = harness.create_spl_token_account(&fixture.subscriber, &spl_usdc_mint, &subscriber);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, mandate.amount);
    let subscriber_wrapped = harness.create_token_2022_ata(&admin, &fixture.mandate, &wrapped_mint_pubkey);
    harness
        .send_wrap(
            &fixture.subscriber,
            &spl_usdc_mint,
            &wrapped_mint_pubkey,
            &subscriber_usdc,
            &subscriber_wrapped,
            &fixture.mandate,
            &wrapping_vault,
            mandate.amount,
        )
        .expect("wrap into periodic mandate should succeed");
    let merchant_wrapped =
        harness.create_token_2022_ata(&admin, &harness.merchant_pubkey(), &wrapped_mint_pubkey);
    let pull_approval = harness.create_pull_approval_with_amount(
        &fixture.mandate,
        harness.current_timestamp() + 600,
        true,
        mandate.amount,
    );
    let caller = harness.create_wallet();

    let err = call_transfer_hook_directly(
        &mut harness,
        &subscriber_wrapped,
        &wrapped_mint_pubkey,
        &merchant_wrapped,
        &fixture.mandate,
        &wrapping_vault,
        &config,
        &pull_approval,
        &token_config,
        mandate.amount,
        &caller,
    )
    .expect_err("periodic mandate owner should fail closed on the stream dispatch path");

    assert!(format!("{:?}", err.err).contains("Custom(6601)"));
}
