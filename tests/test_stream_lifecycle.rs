#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use litesvm::types::{FailedTransactionMetadata, TransactionMetadata};
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_2022::extension::{
    transfer_hook::TransferHookAccount, BaseStateWithExtensions, StateWithExtensions,
};
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

fn error_has(failure: &FailedTransactionMetadata, needle: &str) -> bool {
    format!("{:?}", failure.err).contains(needle)
        || failure.meta.logs.iter().any(|log| log.contains(needle))
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
    let stream: StreamMandate = fixture
        .harness
        .fetch_anchor_account(&fixture.stream_mandate);

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
    let (wrapped_mint_pubkey, _) =
        invalid_interval.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    invalid_interval
        .send_init_merchant_credential()
        .expect("merchant credential bootstrap should succeed");
    let err = invalid_interval
        .send_create_stream_mandate(&subscriber, &wrapped_mint_pubkey, 10, 10, Some(500), 59)
        .expect_err("min_settle_interval < 60 should fail");
    assert!(error_has(&err, "6708") || error_has(&err, "MinSettleIntervalTooLow"));

    let err = invalid_interval
        .send_create_stream_mandate(&subscriber, &wrapped_mint_pubkey, 0, 10, Some(500), 60)
        .expect_err("zero rate should fail");
    assert!(error_has(&err, "6707") || error_has(&err, "RateMustBeNonZero"));

    let err = invalid_interval
        .send_create_stream_mandate(&subscriber, &wrapped_mint_pubkey, 11, 10, Some(500), 60)
        .expect_err("rate above authorized_max_rate should fail");
    assert!(error_has(&err, "6709") || error_has(&err, "AuthorizedMaxRateTooLow"));
}

#[test]
fn test_execute_stream_clamped() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 5_000);
    let source_before = fixture
        .harness
        .fetch_spl_token_account(&fixture.subscriber_wrapped);
    let merchant_before = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);
    let stream_before: StreamMandate = fixture
        .harness
        .fetch_anchor_account(&fixture.stream_mandate);
    let (expected_stream, expected_bump) = Pubkey::find_program_address(
        &[
            StreamMandate::SEED_PREFIX,
            Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()).as_ref(),
            fixture.harness.merchant_pubkey().as_ref(),
            stream_before.mandate_index.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    );
    assert_eq!(
        source_before.owner,
        helpers::to_address(fixture.stream_mandate)
    );
    assert_eq!(
        source_before.mint,
        helpers::to_address(fixture.wrapped_mint)
    );
    assert_eq!(
        merchant_before.mint,
        helpers::to_address(fixture.wrapped_mint)
    );
    assert!(
        fixture
            .harness
            .fetch_account_data(&fixture.subscriber_wrapped)
            .len()
            > 165,
        "source wrapped account should include transfer-hook extensions"
    );
    assert!(
        fixture
            .harness
            .fetch_account_data(&fixture.merchant_wrapped)
            .len()
            > 165,
        "destination wrapped account should include transfer-hook extensions"
    );
    let source_data = fixture
        .harness
        .fetch_account_data(&fixture.subscriber_wrapped);
    let destination_data = fixture
        .harness
        .fetch_account_data(&fixture.merchant_wrapped);
    let source_ext = StateWithExtensions::<spl_token_2022::state::Account>::unpack(&source_data)
        .expect("source wrapped account should unpack with extensions");
    let destination_ext =
        StateWithExtensions::<spl_token_2022::state::Account>::unpack(&destination_data)
            .expect("destination wrapped account should unpack with extensions");
    source_ext
        .get_extension::<TransferHookAccount>()
        .expect("source wrapped account should carry transfer-hook extension");
    destination_ext
        .get_extension::<TransferHookAccount>()
        .expect("destination wrapped account should carry transfer-hook extension");
    assert_eq!(expected_stream, fixture.stream_mandate);
    assert_eq!(stream_before.bump, expected_bump);
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 100);
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

    let mandate: StreamMandate = fixture
        .harness
        .fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);

    assert_eq!(mandate.total_streamed, 1_000);
    assert_eq!(merchant_wrapped.amount, 1_000);
}

#[test]
fn test_execute_stream_cap_clamp() {
    let mut fixture = setup_stream_fixture(10, 10, Some(500), 60, 5_000);
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 100);
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

    let mandate: StreamMandate = fixture
        .harness
        .fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);

    assert_eq!(mandate.total_streamed, 500);
    assert_eq!(merchant_wrapped.amount, 500);
}

#[test]
fn test_hook_misroute_wrong_account_type() {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(
        25_000_000,
        vela_protocol::constants::MIN_FREQUENCY_SECONDS,
        0,
        2,
    );
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);
    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    let config = harness.derive_config();
    let token_config = harness.derive_token_config_address(&wrapped_mint_pubkey);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let subscriber_usdc =
        harness.create_spl_token_account(&fixture.subscriber, &spl_usdc_mint, &subscriber);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, mandate.amount);
    let subscriber_wrapped =
        harness.create_token_2022_ata(&admin, &fixture.mandate, &wrapped_mint_pubkey);
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
    let pull_approval = harness.derive_pull_approval_address(&fixture.mandate);
    harness
        .svm
        .set_account(
            helpers::to_address(pull_approval),
            solana_account::Account {
                lamports: 1_000_000,
                data: vec![9; 8],
                owner: helpers::to_address(vela_protocol::ID),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("wrongly discriminated slot account should be creatable");
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

    assert!(error_has(&err, "6601") || error_has(&err, "WrongAccountType"));
}

#[test]
fn test_pause_requires_active() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 5_000);
    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 100);

    fixture
        .harness
        .send_pause_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("initial pause should succeed");

    let err = fixture
        .harness
        .send_pause_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect_err("second pause should fail once mandate is paused");

    assert!(error_has(&err, "6701") || error_has(&err, "StreamAlreadyPaused"));
}

#[test]
fn test_pause_signer_authorization() {
    let mut subscriber_fixture = setup_stream_fixture(10, 10, None, 60, 5_000);
    subscriber_fixture
        .harness
        .set_clock_timestamp(subscriber_fixture.created_at + 100);
    subscriber_fixture
        .harness
        .send_pause_stream(
            &subscriber_fixture.subscriber,
            &subscriber_fixture.stream_mandate,
            &subscriber_fixture.subscriber_wrapped,
            &subscriber_fixture.merchant_wrapped,
            &subscriber_fixture.wrapped_mint,
        )
        .expect("subscriber should be allowed to pause");

    let mut merchant_fixture = setup_stream_fixture(10, 10, None, 60, 5_000);
    merchant_fixture
        .harness
        .set_clock_timestamp(merchant_fixture.created_at + 100);
    let merchant = merchant_fixture.harness.merchant.insecure_clone();
    merchant_fixture
        .harness
        .send_pause_stream(
            &merchant,
            &merchant_fixture.stream_mandate,
            &merchant_fixture.subscriber_wrapped,
            &merchant_fixture.merchant_wrapped,
            &merchant_fixture.wrapped_mint,
        )
        .expect("merchant should be allowed to pause");

    let mut intruder_fixture = setup_stream_fixture(10, 10, None, 60, 5_000);
    intruder_fixture
        .harness
        .set_clock_timestamp(intruder_fixture.created_at + 100);
    let intruder = intruder_fixture.harness.create_wallet();
    let err = intruder_fixture
        .harness
        .send_pause_stream(
            &intruder,
            &intruder_fixture.stream_mandate,
            &intruder_fixture.subscriber_wrapped,
            &intruder_fixture.merchant_wrapped,
            &intruder_fixture.wrapped_mint,
        )
        .expect_err("random signer should be rejected");

    assert!(error_has(&err, "6710") || error_has(&err, "UnauthorizedStreamSigner"));
}

#[test]
fn test_resume_requires_paused() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 5_000);
    let err = fixture
        .harness
        .send_resume_stream(&fixture.subscriber, &fixture.stream_mandate)
        .expect_err("resume should fail while mandate is still active");

    assert!(error_has(&err, "6700") || error_has(&err, "StreamNotActive"));
}

#[test]
fn test_pause_resume_no_back_accrual() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 50_000);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 100);
    fixture
        .harness
        .send_pause_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("pause should settle the first 100 seconds");

    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 3_700);
    fixture
        .harness
        .send_resume_stream(&fixture.subscriber, &fixture.stream_mandate)
        .expect("resume should reopen the mandate without back-accrual");

    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 3_760);
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
        .expect("execute_stream should only accrue post-resume time");

    let mandate: StreamMandate = fixture
        .harness
        .fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);

    assert_eq!(mandate.total_streamed, 1_600);
    assert_eq!(merchant_wrapped.amount, 1_600);
}

#[test]
fn test_cancel_stream_prorata() {
    let mut fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    fixture.harness.set_clock_timestamp(fixture.created_at + 60);

    fixture
        .harness
        .send_cancel_stream(
            &fixture.subscriber,
            &fixture.stream_mandate,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("cancel should settle accrued amount before finalizing");

    let mandate: StreamMandate = fixture
        .harness
        .fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);

    assert!(matches!(mandate.status, StreamStatus::Cancelled));
    assert_eq!(mandate.total_streamed, 600);
    assert_eq!(merchant_wrapped.amount, 600);
}

#[test]
fn test_cancel_signer_authorization() {
    let mut subscriber_fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    subscriber_fixture
        .harness
        .set_clock_timestamp(subscriber_fixture.created_at + 60);
    subscriber_fixture
        .harness
        .send_cancel_stream(
            &subscriber_fixture.subscriber,
            &subscriber_fixture.stream_mandate,
            &subscriber_fixture.subscriber_wrapped,
            &subscriber_fixture.merchant_wrapped,
            &subscriber_fixture.wrapped_mint,
        )
        .expect("subscriber should be allowed to cancel");

    let mut merchant_fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    merchant_fixture
        .harness
        .set_clock_timestamp(merchant_fixture.created_at + 60);
    let merchant = merchant_fixture.harness.merchant.insecure_clone();
    merchant_fixture
        .harness
        .send_cancel_stream(
            &merchant,
            &merchant_fixture.stream_mandate,
            &merchant_fixture.subscriber_wrapped,
            &merchant_fixture.merchant_wrapped,
            &merchant_fixture.wrapped_mint,
        )
        .expect("merchant should be allowed to cancel");

    let mut intruder_fixture = setup_stream_fixture(10, 10, None, 60, 10_000);
    intruder_fixture
        .harness
        .set_clock_timestamp(intruder_fixture.created_at + 60);
    let intruder = intruder_fixture.harness.create_wallet();
    let err = intruder_fixture
        .harness
        .send_cancel_stream(
            &intruder,
            &intruder_fixture.stream_mandate,
            &intruder_fixture.subscriber_wrapped,
            &intruder_fixture.merchant_wrapped,
            &intruder_fixture.wrapped_mint,
        )
        .expect_err("random signer should be rejected");

    assert!(error_has(&err, "6710") || error_has(&err, "UnauthorizedStreamSigner"));
}

#[test]
fn test_update_rate_settles_first() {
    let mut fixture = setup_stream_fixture(10, 50, None, 60, 10_000);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let merchant = fixture.harness.merchant.insecure_clone();

    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 100);
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
        .expect("merchant should be able to raise within the existing ceiling");

    let after_update: StreamMandate = fixture
        .harness
        .fetch_anchor_account(&fixture.stream_mandate);
    assert_eq!(after_update.total_streamed, 1_000);
    assert_eq!(after_update.rate_per_second, 20);

    fixture
        .harness
        .set_clock_timestamp(fixture.created_at + 160);
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
        .expect("post-update settlement should accrue at the new rate");

    let after_execute: StreamMandate = fixture
        .harness
        .fetch_anchor_account(&fixture.stream_mandate);
    let merchant_wrapped = fixture
        .harness
        .fetch_spl_token_account(&fixture.merchant_wrapped);
    assert_eq!(after_execute.total_streamed, 2_200);
    assert_eq!(merchant_wrapped.amount, 2_200);
}

#[test]
fn test_update_rate_d05_matrix() {
    let mut within_ceiling = setup_stream_fixture(10, 20, None, 60, 10_000);
    within_ceiling
        .harness
        .set_clock_timestamp(within_ceiling.created_at + 60);
    let merchant = within_ceiling.harness.merchant.insecure_clone();
    within_ceiling
        .harness
        .send_update_stream_rate(
            &merchant,
            &within_ceiling.stream_mandate,
            &within_ceiling.subscriber_wrapped,
            &within_ceiling.merchant_wrapped,
            &within_ceiling.wrapped_mint,
            Some(15),
            None,
        )
        .expect("merchant should be able to raise within ceiling");

    let mut above_ceiling = setup_stream_fixture(10, 20, None, 60, 10_000);
    above_ceiling
        .harness
        .set_clock_timestamp(above_ceiling.created_at + 60);
    let merchant = above_ceiling.harness.merchant.insecure_clone();
    let err = above_ceiling
        .harness
        .send_update_stream_rate(
            &merchant,
            &above_ceiling.stream_mandate,
            &above_ceiling.subscriber_wrapped,
            &above_ceiling.merchant_wrapped,
            &above_ceiling.wrapped_mint,
            Some(25),
            None,
        )
        .expect_err("merchant should not be able to exceed the current ceiling");
    assert!(error_has(&err, "6710") || error_has(&err, "UnauthorizedStreamSigner"));

    let mut subscriber_raise = setup_stream_fixture(10, 20, None, 60, 10_000);
    subscriber_raise
        .harness
        .set_clock_timestamp(subscriber_raise.created_at + 60);
    subscriber_raise
        .harness
        .send_update_stream_rate(
            &subscriber_raise.subscriber,
            &subscriber_raise.stream_mandate,
            &subscriber_raise.subscriber_wrapped,
            &subscriber_raise.merchant_wrapped,
            &subscriber_raise.wrapped_mint,
            Some(25),
            Some(25),
        )
        .expect("subscriber should be able to raise both rate and ceiling");

    let mut merchant_ceiling = setup_stream_fixture(10, 20, None, 60, 10_000);
    merchant_ceiling
        .harness
        .set_clock_timestamp(merchant_ceiling.created_at + 60);
    let merchant = merchant_ceiling.harness.merchant.insecure_clone();
    let err = merchant_ceiling
        .harness
        .send_update_stream_rate(
            &merchant,
            &merchant_ceiling.stream_mandate,
            &merchant_ceiling.subscriber_wrapped,
            &merchant_ceiling.merchant_wrapped,
            &merchant_ceiling.wrapped_mint,
            None,
            Some(25),
        )
        .expect_err("merchant should not be able to change the ceiling");
    assert!(error_has(&err, "6710") || error_has(&err, "UnauthorizedStreamSigner"));

    let mut subscriber_ceiling = setup_stream_fixture(10, 20, None, 60, 10_000);
    subscriber_ceiling
        .harness
        .set_clock_timestamp(subscriber_ceiling.created_at + 60);
    subscriber_ceiling
        .harness
        .send_update_stream_rate(
            &subscriber_ceiling.subscriber,
            &subscriber_ceiling.stream_mandate,
            &subscriber_ceiling.subscriber_wrapped,
            &subscriber_ceiling.merchant_wrapped,
            &subscriber_ceiling.wrapped_mint,
            None,
            Some(30),
        )
        .expect("subscriber should be able to raise the ceiling only");
}
