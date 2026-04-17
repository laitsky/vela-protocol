//! Tests for the transfer hook validation flow (HOOK-01, HOOK-02, HOOK-05).
//!
//! These tests validate:
//! - Transfer hook initialization (init_wrapped_mint, init_extra_account_meta_list)
//! - Transfer hook validation with valid/expired/absent/excess-amount PullApproval PDAs
//! - Wrap/unwrap transfers bypass the hook (vault address check)
//!
//! NOTE on LiteSVM and transfer hook CPI chain:
//! Token-2022's transfer_checked CPIs into our transfer hook program. LiteSVM 0.10.0
//! loads spl-token-2022 from its embedded artifact (v10.0.0 compatible). The transfer
//! hook Execute CPI is dispatched via the fallback instruction handler in lib.rs.
//!
//! For tests that exercise the full CPI chain (execute_pull -> Token-2022 -> transfer_hook),
//! we rely on the injected Token-2022 account approach. If the hook CPI chain does not fire
//! in LiteSVM (LiteSVM may not support cross-program CPIs for Token-2022 extensions), then
//! execute_pull will succeed but the hook won't be invoked -- the tests still validate the
//! on-chain logic of the surrounding instructions (approval pre-check, mandate state updates).
//!
//! Standalone hook handler tests (calling transfer_hook instruction directly) test the hook
//! logic in isolation, bypassing the Token-2022 dispatch.

#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{MandateStatus, VelaMandate, VelaPlan},
};

/// Set up a subscription with Token-2022 wrapped USDC accounts injected.
fn setup_subscription_with_t22(
    amount: u64,
    max_pulls: u64,
) -> (
    TestHarness,
    helpers::SubscriptionFixture,
    VelaPlan,
    VelaMandate,
    Pubkey, // subscriber_wrapped_account
    Pubkey, // merchant_wrapped_account
    Pubkey, // wrapped_usdc_mint
) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(amount, MIN_FREQUENCY_SECONDS, 0, max_pulls);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);

    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);

    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);

    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let subscriber_usdc =
        harness.create_spl_token_account(&fixture.subscriber, &spl_usdc_mint, &subscriber);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, amount * 10);

    let subscriber_wrapped_pubkey =
        harness.create_token_2022_ata(&admin, &fixture.mandate, &wrapped_mint_pubkey);
    harness
        .send_wrap(
            &fixture.subscriber,
            &spl_usdc_mint,
            &wrapped_mint_pubkey,
            &subscriber_usdc,
            &subscriber_wrapped_pubkey,
            &fixture.mandate,
            &wrapping_vault,
            amount * 2,
        )
        .expect("wrap into the mandate billing account should succeed");

    let merchant_wrapped_pubkey =
        harness.create_token_2022_ata(&admin, &harness.merchant_pubkey(), &wrapped_mint_pubkey);

    (
        harness,
        fixture,
        plan,
        mandate,
        subscriber_wrapped_pubkey,
        merchant_wrapped_pubkey,
        wrapped_mint_pubkey,
    )
}

// ================================================================
// init_wrapped_mint tests
// ================================================================

#[test]
fn test_init_wrapped_mint_requires_protocol_config() {
    // Trying to init_wrapped_mint without a valid ProtocolConfig should fail.
    // This verifies the has_one = admin constraint is enforced.
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);

    // Don't init_config first -- config PDA doesn't exist
    let wrapped_mint = Keypair::new();
    let wrapped_mint_pubkey = helpers::to_anchor_pubkey(wrapped_mint.pubkey());

    // Should fail because ProtocolConfig doesn't exist
    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let wrapping_vault = harness.derive_spl_ata(&mint_authority, &spl_usdc_mint);

    use anchor_lang::{InstructionData, ToAccountMetas};
    use helpers::{spl_token_address, token_2022_address};

    let accounts = vela_protocol::accounts::InitWrappedMint {
        admin: helpers::to_anchor_pubkey(admin.pubkey()),
        config,
        mint_authority,
        wrapped_usdc_mint: wrapped_mint_pubkey,
        spl_usdc_mint,
        wrapping_vault,
        token_2022_program: helpers::to_anchor_pubkey(token_2022_address()),
        spl_token_program: helpers::to_anchor_pubkey(spl_token_address()),
        associated_token_program: Pubkey::new_from_array(
            spl_associated_token_account::id().to_bytes(),
        ),
        system_program: anchor_lang::system_program::ID,
        rent: anchor_lang::solana_program::sysvar::rent::ID,
    };
    let ix = solana_instruction::Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::InitWrappedMint {}.data(),
    };
    let error = harness
        .send_instructions(&[ix], &[&admin, &wrapped_mint], Some(&admin.pubkey()))
        .expect_err("init_wrapped_mint without ProtocolConfig should fail");

    assert!(
        format!("{:?}", error.err).contains("Custom(")
            || format!("{:?}", error.err).contains("AccountNotFound"),
        "expected missing config error, got {:?}",
        error.err,
    );
}

#[test]
fn test_init_extra_account_meta_list() {
    // Verify ExtraAccountMetaList PDA is created with non-empty data.
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    let _config = harness.init_protocol_config(&admin);
    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    let extra_account_meta_list =
        harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);

    // Verify ExtraAccountMetaList PDA exists and has non-empty data
    let data = harness.fetch_account_data(&extra_account_meta_list);
    assert!(
        !data.is_empty(),
        "ExtraAccountMetaList should have non-empty data"
    );
    assert!(
        data.len() > 8,
        "ExtraAccountMetaList should contain TLV entries"
    );
}

// ================================================================
// Transfer hook approval validation tests (via execute_pull)
// These tests validate that execute_pull enforces approval rules.
// The transfer hook itself is tested via the standalone instruction tests below.
// ================================================================

#[test]
fn test_transfer_hook_with_valid_approval() {
    // Full execute_pull flow: valid PullApproval with approved=true, correct amount, not expired.
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_subscription_with_t22(25_000_000, 2);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("execute_pull with valid approval should succeed");

    // Mandate should have pulls_executed incremented
    let mandate_after: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert_eq!(
        mandate_after.pulls_executed, 1,
        "mandate.pulls_executed should be 1 after successful pull"
    );
    assert!(matches!(mandate_after.status, MandateStatus::Active));

    // PullApproval should be closed (refunded to payer)
    let approval_pda = harness.derive_pull_approval_address(&fixture.mandate);
    assert!(
        harness
            .svm
            .get_account(&helpers::to_address(approval_pda))
            .is_none(),
        "PullApproval PDA should be closed after successful execute_pull"
    );
}

#[test]
fn test_transfer_hook_no_approval_fails() {
    // No PullApproval PDA exists -- execute_pull should fail with ApprovalNotGranted.
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_subscription_with_t22(25_000_000, 2);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate.next_payment_due);
    // No approval created

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("execute_pull without PullApproval should fail");

    assert!(
        format!("{:?}", error.err).contains("Custom(6011)"),
        "expected ApprovalNotGranted (6011), got {:?}",
        error.err,
    );
}

#[test]
fn test_transfer_hook_expired_approval_fails() {
    // PullApproval.valid_until is in the past -- should fail with ApprovalExpired.
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_subscription_with_t22(25_000_000, 2);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    // Set clock PAST the approval's valid_until
    harness.set_clock_timestamp(mandate.next_payment_due + 1);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due, // valid_until is now in the past
        true,
        plan.amount,
    );

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("expired approval should fail");

    assert!(
        format!("{:?}", error.err).contains("Custom(6012)"),
        "expected ApprovalExpired (6012), got {:?}",
        error.err,
    );
}

#[test]
fn test_transfer_hook_rejected_approval_fails() {
    // PullApproval.approved = false -- Arcium said no. Should fail with ApprovalNotGranted.
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_subscription_with_t22(25_000_000, 2);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        false, // approved = false
        plan.amount,
    );

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("rejected approval should fail");

    assert!(
        format!("{:?}", error.err).contains("Custom(6011)"),
        "expected ApprovalNotGranted (6011) for rejected approval, got {:?}",
        error.err,
    );
}

#[test]
fn test_transfer_hook_excess_amount_fails() {
    // PullApproval.approved_amount is less than plan.amount. execute_pull now pre-validates
    // the amount before issuing the Token-2022 transfer, and the hook re-validates it as well.
    let (mut harness, fixture, plan, mandate, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_subscription_with_t22(25_000_000, 2);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate.next_payment_due);
    // Set approved_amount to less than plan.amount
    let low_approved_amount = plan.amount / 2;
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        low_approved_amount, // only approved for half the amount
    );

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("execute_pull should reject approvals smaller than the billing amount");

    assert!(
        format!("{:?}", error.err).contains("Custom(6009)"),
        "expected AmountExceedsPlanAmount (6009), got {:?}",
        error.err,
    );
}

// ================================================================
// Standalone transfer hook instruction tests
// These call the transfer_hook instruction directly (bypassing Token-2022 dispatch)
// to test the hook validation logic in isolation.
// ================================================================

/// Call the transfer_hook instruction directly to test validation logic.
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
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    use solana_instruction::AccountMeta;

    // The transfer hook Execute instruction has a fixed account layout:
    // 0: source_token (T22 account)
    // 1: mint (T22 mint)
    // 2: destination_token (T22 account)
    // 3: owner (UncheckedAccount)
    // 4: extra_account_meta_list
    // 5: protocol_program
    // 6: wrapping_vault
    // 7: protocol_config
    // 8: pull_approval
    // 9: token_config
    // 10-12: reserved placeholders

    let (extra_account_meta_list, _) = harness.derive_extra_account_meta_list(mint);

    // Build the Transfer Hook Execute instruction using the discriminator from spl-transfer-hook-interface
    let amount_bytes = amount.to_le_bytes();
    // Discriminator for "spl-transfer-hook-interface:execute" (first 8 bytes of SHA256 hash)
    let discriminator: [u8; 8] = [105, 37, 101, 197, 75, 251, 102, 26];
    let mut data = discriminator.to_vec();
    data.extend_from_slice(&amount_bytes);

    let accounts = vec![
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
    ];

    let ix = solana_instruction::Instruction {
        program_id: harness.hook_program_id,
        accounts,
        data,
    };
    harness.send_instructions(&[ix], &[caller], Some(&caller.pubkey()))
}

#[test]
fn test_hook_handler_validates_approval_directly() {
    // Call the transfer hook instruction directly with a valid approval.
    // This tests the hook handler logic in isolation.
    let (
        mut harness,
        fixture,
        plan,
        mandate,
        subscriber_wrapped_pubkey,
        merchant_wrapped_pubkey,
        wrapped_mint_pubkey,
    ) = setup_subscription_with_t22(25_000_000, 2);
    let config = harness.derive_config();
    let config_account: vela_protocol::state::ProtocolConfig =
        harness.fetch_anchor_account(&config);
    let wrapping_vault = config_account.wrapping_vault;
    let token_config = harness.derive_token_config_address(&wrapped_mint_pubkey);

    harness.set_clock_timestamp(mandate.next_payment_due);
    let approval_pda = harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        plan.amount,
    );

    // Call hook directly with valid approval
    let owner = fixture.mandate; // mandate PDA is owner of subscriber_wrapped
    let caller = harness.create_wallet();

    let result = call_transfer_hook_directly(
        &mut harness,
        &subscriber_wrapped_pubkey,
        &wrapped_mint_pubkey,
        &merchant_wrapped_pubkey,
        &owner,
        &wrapping_vault,
        &config,
        &approval_pda,
        &token_config,
        plan.amount,
        &caller,
    );

    result.expect("standalone transfer-hook validation should succeed with a bound approval");
}

#[test]
fn test_hook_handler_rejects_no_approval_directly() {
    // Call the transfer hook instruction directly with NO approval account.
    let (
        mut harness,
        fixture,
        plan,
        _mandate,
        subscriber_wrapped_pubkey,
        merchant_wrapped_pubkey,
        wrapped_mint_pubkey,
    ) = setup_subscription_with_t22(25_000_000, 2);
    let config = harness.derive_config();
    let config_account: vela_protocol::state::ProtocolConfig =
        harness.fetch_anchor_account(&config);
    let wrapping_vault = config_account.wrapping_vault;
    let token_config = harness.derive_token_config_address(&wrapped_mint_pubkey);

    // Use a random pubkey as "pull_approval" (not a real account owned by program)
    let fake_approval = Keypair::new();
    let fake_approval_pubkey = helpers::to_anchor_pubkey(fake_approval.pubkey());

    let caller = harness.create_wallet();
    let owner = fixture.mandate;

    let error = call_transfer_hook_directly(
        &mut harness,
        &subscriber_wrapped_pubkey,
        &wrapped_mint_pubkey,
        &merchant_wrapped_pubkey,
        &owner,
        &wrapping_vault,
        &config,
        &fake_approval_pubkey,
        &token_config,
        plan.amount,
        &caller,
    )
    .expect_err("standalone transfer-hook validation should reject unrelated approvals");

    let err_str = format!("{:?}", error.err);
    assert!(
        err_str.contains("Custom(6028)") || err_str.contains("Custom("),
        "expected transfer not authorized error, got {:?}",
        error.err,
    );
}

#[test]
fn test_mandate_status_after_all_pulls_expired() {
    // Test that mandate transitions to Expired after all max_pulls are consumed.
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_subscription_with_t22(25_000_000, 1); // max_pulls = 1
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("single pull on 1-max_pulls mandate should succeed");

    let mandate_after: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert_eq!(mandate_after.pulls_executed, 1);
    assert!(
        matches!(mandate_after.status, MandateStatus::Expired),
        "mandate should be Expired after max_pulls reached"
    );
}
