#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::{prelude::Pubkey, InstructionData, ToAccountMetas};
use helpers::TestHarness;
use solana_instruction::{Instruction, AccountMeta};
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    errors::VelaError,
    state::{MerchantState, ProtocolConfig, StreamMandate, StreamStatus},
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
        created_stream.last_settled_ts + 60 * 60 * 24 * 8,
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

fn error_has(metadata: &litesvm::types::FailedTransactionMetadata, needle: &str) -> bool {
    format!("{:?}", metadata.err).contains(needle)
        || metadata.meta.logs.iter().any(|log| log.contains(needle))
}

fn assert_anchor_error(
    metadata: &litesvm::types::FailedTransactionMetadata,
    error: VelaError,
) {
    let code = error as u32 + anchor_lang::error::ERROR_CODE_OFFSET;
    assert!(
        error_has(metadata, &format!("Custom({code})")) || error_has(metadata, &code.to_string()),
        "expected anchor error {error:?} ({code}), got err={:?}, logs={:?}",
        metadata.err,
        metadata.meta.logs
    );
}

fn execute_stream_ix(
    fixture: &StreamFixture,
    payer: &Keypair,
    subscriber: &Pubkey,
) -> Instruction {
    let config = fixture.harness.derive_config();
    let config_account: ProtocolConfig = fixture.harness.fetch_anchor_account(&config);
    let (extra_account_meta_list, _) = fixture
        .harness
        .derive_extra_account_meta_list(&fixture.wrapped_mint);
    let keeper_config = fixture.harness.derive_keeper_config().0;
    let accounts = vela_protocol::accounts::ExecuteStream {
        payer: Pubkey::new_from_array(payer.pubkey().to_bytes()),
        subscriber: *subscriber,
        merchant: fixture.harness.merchant_pubkey(),
        keeper_config,
        stream_mandate: fixture.stream_mandate,
        subscriber_wrapped_account: fixture.subscriber_wrapped,
        merchant_wrapped_account: fixture.merchant_wrapped,
        wrapped_usdc_mint: fixture.wrapped_mint,
        pull_approval: fixture
            .harness
            .derive_pull_approval_address(&fixture.stream_mandate),
        token_config: fixture.harness.derive_token_config_address(&fixture.wrapped_mint),
        protocol_config: config,
        wrapping_vault: config_account.wrapping_vault,
        hook_program: Pubkey::new_from_array(vela_transfer_hook::ID.to_bytes()),
        extra_account_meta_list,
        protocol_program: vela_protocol::ID,
        token_2022_program: anchor_spl::token_2022::ID,
        system_program: anchor_lang::system_program::ID,
    };

    Instruction {
        program_id: fixture.harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect::<Vec<AccountMeta>>(),
        data: vela_protocol::instruction::ExecuteStream {}.data(),
    }
}

fn update_stream_rate_ix(
    fixture: &StreamFixture,
    authority: &Keypair,
    new_rate: Option<u64>,
    new_authorized_max_rate: Option<u64>,
) -> Instruction {
    let config = fixture.harness.derive_config();
    let config_account: ProtocolConfig = fixture.harness.fetch_anchor_account(&config);
    let (extra_account_meta_list, _) = fixture
        .harness
        .derive_extra_account_meta_list(&fixture.wrapped_mint);
    let accounts = vela_protocol::accounts::UpdateStreamRate {
        authority: Pubkey::new_from_array(authority.pubkey().to_bytes()),
        mandate: fixture.stream_mandate,
        subscriber_wrapped_account: fixture.subscriber_wrapped,
        merchant_wrapped_account: fixture.merchant_wrapped,
        wrapped_usdc_mint: fixture.wrapped_mint,
        pull_approval: fixture
            .harness
            .derive_pull_approval_address(&fixture.stream_mandate),
        token_config: fixture.harness.derive_token_config_address(&fixture.wrapped_mint),
        protocol_config: config,
        wrapping_vault: config_account.wrapping_vault,
        hook_program: Pubkey::new_from_array(vela_transfer_hook::ID.to_bytes()),
        extra_account_meta_list,
        protocol_program: vela_protocol::ID,
        token_2022_program: anchor_spl::token_2022::ID,
        system_program: anchor_lang::system_program::ID,
    };

    Instruction {
        program_id: fixture.harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect::<Vec<AccountMeta>>(),
        data: vela_protocol::instruction::UpdateStreamRate {
            new_rate,
            new_authorized_max_rate,
        }
        .data(),
    }
}

#[test]
fn test_long_keeper_downtime() {
    let mut fixture = setup_stream_fixture(100, 100, None, 60, 100_000_000);
    let keeper = fixture.subscriber.insecure_clone();
    fixture.harness.ensure_keeper_config(&keeper);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    fixture.harness.set_clock_timestamp(fixture.created_at + 7 * 86_400);
    fixture
        .harness
        .send_execute_stream(
            &keeper,
            &subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("single settlement after keeper downtime should succeed");

    let mandate: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);

    assert_eq!(mandate.total_streamed, 60_480_000);
    assert_eq!(merchant_wrapped.amount, 60_480_000);
}

#[test]
fn test_rate_change_mid_stream() {
    let mut fixture = setup_stream_fixture(10, 50, None, 60, 10_000);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let merchant = fixture.harness.merchant.insecure_clone();

    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    fixture
        .harness
        .send_update_stream_rate(
            &merchant,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
            Some(20),
            None,
        )
        .expect("rate update should settle then mutate");

    fixture.harness.set_clock_timestamp(fixture.created_at + 200);
    fixture
        .harness
        .send_execute_stream(
            &merchant,
            &subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("post-update settlement should succeed");

    let mandate: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);
    assert_eq!(mandate.total_streamed, 3_000);
    assert_eq!(merchant_wrapped.amount, 3_000);
}

#[test]
fn test_cap_reached_mid_settle() {
    let mut fixture = setup_stream_fixture(10, 10, Some(500), 60, 10_000);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let merchant = fixture.harness.merchant.insecure_clone();

    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    fixture
        .harness
        .send_execute_stream(
            &merchant,
            &subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("first settlement should clamp to cap");

    let after_first: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    let merchant_after_first = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);
    assert_eq!(after_first.total_streamed, 500);
    assert_eq!(merchant_after_first.amount, 500);

    fixture.harness.set_clock_timestamp(fixture.created_at + 200);
    let second = fixture.harness.send_execute_stream(
        &merchant,
        &subscriber,
        &fixture.stream_mandate,
        &fixture.subscriber_wrapped,
        &fixture.merchant_wrapped,
        &fixture.wrapped_mint,
    );

    match second {
        Ok(_) => {
            let after_second: StreamMandate =
                fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
            let merchant_after_second = fixture
                .harness
                .fetch_spl_token_account(&fixture.merchant_wrapped);
            assert_eq!(after_second.total_streamed, 500);
            assert_eq!(merchant_after_second.amount, 500);
        }
        Err(err) => {
            let after_second: StreamMandate =
                fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
            assert_eq!(after_second.total_streamed, 500);
            assert!(
                error_has(&err, "6700")
                    || error_has(&err, "6703")
                    || error_has(&err, "6705")
                    || error_has(&err, "Cap")
            );
        }
    }
}

#[test]
fn test_clock_regression_rejected() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let merchant = fixture.harness.merchant.insecure_clone();

    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    let mut mandate: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    mandate.last_settled_ts = fixture.created_at + 200;
    fixture
        .harness
        .overwrite_anchor_account(&fixture.stream_mandate, &mandate);

    let err = fixture
        .harness
        .send_execute_stream(
            &merchant,
            &subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect_err("clock regression should be rejected");

    assert_anchor_error(&err, VelaError::ClockRegression);

    let after: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    assert_eq!(after.last_settled_ts, fixture.created_at + 200);
    assert_eq!(after.total_streamed, 0);
}

#[test]
fn test_pause_resume_cancel_sequence() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 50_000);

    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    fixture
        .harness
        .send_pause_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("pause should settle accrued stream");

    let paused: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    assert!(matches!(paused.status, StreamStatus::Paused));
    assert_eq!(paused.total_streamed, 1_000);

    fixture.harness.set_clock_timestamp(fixture.created_at + 3_700);
    fixture
        .harness
        .send_resume_stream(&fixture.subscriber, &fixture.stream_mandate)
        .expect("resume should reactivate mandate");

    let resumed: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    assert!(matches!(resumed.status, StreamStatus::Active));
    assert_eq!(resumed.total_streamed, 1_000);

    fixture.harness.set_clock_timestamp(fixture.created_at + 3_760);
    fixture
        .harness
        .send_cancel_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("cancel should settle only post-resume accrual");

    let cancelled: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);
    assert!(matches!(cancelled.status, StreamStatus::Cancelled));
    assert_eq!(cancelled.total_streamed, 1_600);
    assert_eq!(merchant_wrapped.amount, 1_600);
}

#[test]
fn test_concurrent_settle_and_update_rate() {
    let mut fixture = setup_stream_fixture(10, 20, None, 60, 10_000);
    let merchant = fixture.harness.merchant.insecure_clone();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    fixture.harness.set_clock_timestamp(fixture.created_at + 100);
    let execute_ix = execute_stream_ix(&fixture, &merchant, &subscriber);
    let update_ix = update_stream_rate_ix(&fixture, &merchant, Some(20), None);

    let result =
        fixture
            .harness
            .send_instructions(&[execute_ix, update_ix], &[&merchant], Some(&merchant.pubkey()));

    match result {
        Ok(_) => {
            let mandate: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
            let merchant_wrapped = fixture
                .harness
                .fetch_spl_token_account(&fixture.merchant_wrapped);
            assert_eq!(mandate.total_streamed, 1_000);
            assert_eq!(mandate.rate_per_second, 20);
            assert_eq!(merchant_wrapped.amount, 1_000);
        }
        Err(_) => {
            let mandate: StreamMandate = fixture.harness.fetch_anchor_account(&fixture.stream_mandate);
            let merchant_wrapped = fixture
                .harness
                .fetch_spl_token_account(&fixture.merchant_wrapped);
            assert_eq!(mandate.total_streamed, 0);
            assert_eq!(mandate.rate_per_second, 10);
            assert_eq!(merchant_wrapped.amount, 0);
        }
    }
}
