#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use litesvm::types::{FailedTransactionMetadata, TransactionMetadata};
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_2022::error::TokenError;
use vela_protocol::{
    errors::VelaError,
    state::{MerchantState, StreamMandate},
};

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
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);
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

fn assert_custom_error_code(failure: &FailedTransactionMetadata, code: u32) {
    let needle = format!("Custom({code})");
    let err = format!("{:?}", failure.err);
    assert!(
        err.contains(&needle) || failure.meta.logs.iter().any(|log| log.contains(&needle)),
        "expected {needle}, got err={err}, logs={:?}",
        failure.meta.logs
    );
}

#[test]
fn test_insufficient_balance_errors() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 100);
    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    let err = fixture
        .harness
        .send_execute_stream(
            &fixture.subscriber,
            &subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect_err("execute_stream should fail closed when wrapped balance is insufficient");

    assert_custom_error_code(&err, TokenError::InsufficientFunds as u32);
}

#[test]
fn test_min_settle_interval_violation() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 5_000);
    fixture.harness.set_clock_timestamp(fixture.created_at + 30);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    let err = fixture
        .harness
        .send_execute_stream(
            &fixture.subscriber,
            &subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect_err("execute_stream should reject settlements before min_settle_interval");

    assert_custom_error_code(&err, VelaError::MinSettleIntervalViolation as u32);
}

#[test]
fn test_cap_clamp_before_hook() {
    let mut fixture = setup_stream_fixture(10, 10, Some(1), 60, 5_000);
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
        .expect("execute_stream should clamp to the remaining cap before the hook");

    let mandate: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);

    assert_eq!(mandate.total_streamed, 1);
    assert_eq!(merchant_wrapped.amount, 1);
}

#[test]
fn test_hook_rejects_after_clamp_bypass() {
    let mut fixture = setup_stream_fixture(10, 10, Some(5), 60, 5_000);
    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    let config = fixture.harness.derive_config();
    let config_account = fixture
        .harness
        .fetch_anchor_account::<vela_protocol::state::ProtocolConfig>(&config);
    let token_config = fixture
        .harness
        .derive_token_config_address(&fixture.wrapped_mint);
    let pull_approval = fixture
        .harness
        .derive_pull_approval_address(&fixture.stream_mandate);
    let caller = fixture.harness.create_wallet();

    let err = call_transfer_hook_directly(
        &mut fixture.harness,
        &fixture.subscriber_wrapped,
        &fixture.wrapped_mint,
        &fixture.merchant_wrapped,
        &fixture.stream_mandate,
        &config_account.wrapping_vault,
        &config,
        &pull_approval,
        &token_config,
        1_001,
        &caller,
    )
    .expect_err("hook should independently reject stream amounts above elapsed * rate");

    assert_custom_error_code(&err, VelaError::AmountExceedsStreamRate as u32);
}
