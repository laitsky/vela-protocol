//! Tests for wrap (SPL USDC -> wrapped USDC) and unwrap (wrapped USDC -> SPL USDC) instructions.
//!
//! These tests verify HOOK-03: the custom protocol vault wrapping mechanism.
//! wrap and unwrap go through the real on-chain instructions via the compiled SBF artifact.

#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_keypair::Keypair;
use solana_signer::Signer;

/// Helper: send init_config to bootstrap ProtocolConfig.
fn init_config(harness: &mut TestHarness, admin: &Keypair) -> Pubkey {
    harness.init_protocol_config(admin)
}

/// Helper: send init_wrapped_mint instruction.
fn init_wrapped_mint(
    harness: &mut TestHarness,
    admin: &Keypair,
    wrapped_mint: &Keypair,
    spl_usdc_mint: &Pubkey,
) -> (Pubkey, Pubkey) {
    harness.init_wrapped_mint(admin, wrapped_mint, spl_usdc_mint)
}

/// Helper: send wrap instruction.
fn send_wrap(
    harness: &mut TestHarness,
    subscriber: &Keypair,
    spl_usdc_mint: &Pubkey,
    wrapped_usdc_mint: &Pubkey,
    subscriber_usdc_account: &Pubkey,
    destination_wrapped_account: &Pubkey,
    destination_authority: &Pubkey,
    wrapping_vault: &Pubkey,
    amount: u64,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    use anchor_lang::{InstructionData, ToAccountMetas};
    use helpers::{spl_token_address, token_2022_address};

    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let subscriber_pubkey = helpers::to_anchor_pubkey(subscriber.pubkey());

    let accounts = vela_protocol::accounts::Wrap {
        subscriber: subscriber_pubkey,
        config,
        spl_usdc_mint: *spl_usdc_mint,
        wrapped_usdc_mint: *wrapped_usdc_mint,
        subscriber_usdc_account: *subscriber_usdc_account,
        destination_wrapped_account: *destination_wrapped_account,
        destination_authority: *destination_authority,
        wrapping_vault: *wrapping_vault,
        mint_authority,
        spl_token_program: helpers::to_anchor_pubkey(spl_token_address()),
        token_2022_program: helpers::to_anchor_pubkey(token_2022_address()),
    };
    let ix = solana_instruction::Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::Wrap { amount }.data(),
    };
    harness.send_instructions(&[ix], &[subscriber], Some(&subscriber.pubkey()))
}

/// Helper: send unwrap instruction.
fn send_unwrap(
    harness: &mut TestHarness,
    user: &Keypair,
    spl_usdc_mint: &Pubkey,
    wrapped_usdc_mint: &Pubkey,
    user_wrapped_account: &Pubkey,
    user_usdc_account: &Pubkey,
    wrapping_vault: &Pubkey,
    amount: u64,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    use anchor_lang::{InstructionData, ToAccountMetas};
    use helpers::{spl_token_address, token_2022_address};

    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let user_pubkey = helpers::to_anchor_pubkey(user.pubkey());

    let accounts = vela_protocol::accounts::Unwrap {
        user: user_pubkey,
        config,
        spl_usdc_mint: *spl_usdc_mint,
        wrapped_usdc_mint: *wrapped_usdc_mint,
        user_wrapped_account: *user_wrapped_account,
        user_usdc_account: *user_usdc_account,
        wrapping_vault: *wrapping_vault,
        mint_authority,
        spl_token_program: helpers::to_anchor_pubkey(spl_token_address()),
        token_2022_program: helpers::to_anchor_pubkey(token_2022_address()),
    };
    let ix = solana_instruction::Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::UnwrapTokens { amount }.data(),
    };
    harness.send_instructions(&[ix], &[user], Some(&user.pubkey()))
}

/// Common test setup: init config, create SPL USDC mint, init wrapped mint.
fn setup_wrap_test() -> (
    TestHarness,
    Keypair, // admin
    Pubkey,  // spl_usdc_mint
    Pubkey,  // wrapped_usdc_mint
    Pubkey,  // wrapping_vault
) {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();

    // Create a mock SPL USDC mint
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);

    // Bootstrap ProtocolConfig
    init_config(&mut harness, &admin);

    // Initialize wrapped USDC mint
    let wrapped_mint_keypair = Keypair::new();
    let (wrapped_usdc_mint, wrapping_vault) =
        init_wrapped_mint(&mut harness, &admin, &wrapped_mint_keypair, &spl_usdc_mint);

    (harness, admin, spl_usdc_mint, wrapped_usdc_mint, wrapping_vault)
}

#[test]
fn test_wrap_before_mint_initialized_fails() {
    // Wrap without init_wrapped_mint should fail (config.wrapped_usdc_mint == default)
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = helpers::to_anchor_pubkey(subscriber.pubkey());

    // Bootstrap config (without init_wrapped_mint)
    init_config(&mut harness, &admin);

    let subscriber_usdc = harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    // admin is the mint authority (create_spl_mint sets authority to admin)
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, 1_000_000);

    // Use a random keypair as a fake wrapped mint (not initialized in config)
    let fake_mint = Keypair::new();
    let fake_mint_pubkey = helpers::to_anchor_pubkey(fake_mint.pubkey());
    harness.inject_token_2022_mint(&fake_mint_pubkey, &subscriber_pubkey, 0);

    let subscriber_wrapped = harness.derive_token_2022_ata(&subscriber_pubkey, &fake_mint_pubkey);
    let (mint_authority, _) = harness.derive_mint_authority();
    let fake_vault = harness.derive_spl_ata(&mint_authority, &spl_usdc_mint);

    // Attempt wrap -- should fail because config.wrapped_usdc_mint != fake_mint
    let error = send_wrap(
        &mut harness,
        &subscriber,
        &spl_usdc_mint,
        &fake_mint_pubkey,
        &subscriber_usdc,
        &subscriber_wrapped,
        &subscriber_pubkey,
        &fake_vault,
        1_000,
    )
    .expect_err("wrap should fail when wrapped mint not in config");

    assert!(
        format!("{:?}", error.err).contains("Custom(") || format!("{:?}", error.err).contains("ConstraintAddress"),
        "expected address constraint or WrappedMintNotInitialized error, got {:?}",
        error.err,
    );
}

#[test]
fn test_wrap_spl_usdc_to_wrapped() {
    let (mut harness, admin, spl_usdc_mint, wrapped_usdc_mint, wrapping_vault) = setup_wrap_test();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = helpers::to_anchor_pubkey(subscriber.pubkey());

    let wrap_amount = 1_000_000; // 1 USDC

    // Create subscriber's SPL USDC account and fund it
    let subscriber_usdc = harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, wrap_amount * 2);

    // Create the real Token-2022 ATA so account extensions match the wrapped mint.
    let subscriber_wrapped =
        harness.create_token_2022_ata(&subscriber, &subscriber_pubkey, &wrapped_usdc_mint);

    // Execute wrap instruction
    send_wrap(
        &mut harness,
        &subscriber,
        &spl_usdc_mint,
        &wrapped_usdc_mint,
        &subscriber_usdc,
        &subscriber_wrapped,
        &subscriber_pubkey,
        &wrapping_vault,
        wrap_amount,
    )
    .expect("wrap should succeed");

    // Verify SPL USDC balance decreased
    let sub_usdc_after = harness.fetch_spl_token_account(&subscriber_usdc);
    assert_eq!(sub_usdc_after.amount, wrap_amount, "subscriber SPL USDC should decrease by wrap_amount");

    // Verify vault received SPL USDC
    let vault_after = harness.fetch_spl_token_account(&wrapping_vault);
    assert_eq!(vault_after.amount, wrap_amount, "vault should receive wrap_amount SPL USDC");
}

#[test]
fn test_unwrap_wrapped_to_spl() {
    let (mut harness, admin, spl_usdc_mint, wrapped_usdc_mint, wrapping_vault) = setup_wrap_test();
    let user = harness.create_wallet();
    let user_pubkey = helpers::to_anchor_pubkey(user.pubkey());

    let wrap_amount = 1_000_000u64;
    let unwrap_amount = 500_000u64;

    // Fund user with SPL USDC and wrap first
    let user_usdc = harness.create_spl_token_account(&user, &spl_usdc_mint, &user_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &user_usdc, wrap_amount);
    // User's wrapped balance must live in a real Token-2022 ATA.
    let user_wrapped = harness.create_token_2022_ata(&user, &user_pubkey, &wrapped_usdc_mint);

    send_wrap(
        &mut harness,
        &user,
        &spl_usdc_mint,
        &wrapped_usdc_mint,
        &user_usdc,
        &user_wrapped,
        &user_pubkey,
        &wrapping_vault,
        wrap_amount,
    )
    .expect("wrap should succeed before unwrap");

    // Now unwrap half
    send_unwrap(
        &mut harness,
        &user,
        &spl_usdc_mint,
        &wrapped_usdc_mint,
        &user_wrapped,
        &user_usdc,
        &wrapping_vault,
        unwrap_amount,
    )
    .expect("unwrap should succeed");

    // Verify vault SPL USDC decreased
    let vault_after = harness.fetch_spl_token_account(&wrapping_vault);
    assert_eq!(
        vault_after.amount,
        wrap_amount - unwrap_amount,
        "vault balance should decrease after unwrap"
    );

    // Verify user's SPL USDC increased
    let user_usdc_after = harness.fetch_spl_token_account(&user_usdc);
    assert_eq!(
        user_usdc_after.amount,
        unwrap_amount,
        "user SPL USDC should increase by unwrap_amount"
    );
}

#[test]
fn test_wrap_insufficient_balance_fails() {
    let (mut harness, admin, spl_usdc_mint, wrapped_usdc_mint, wrapping_vault) = setup_wrap_test();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = helpers::to_anchor_pubkey(subscriber.pubkey());

    // Create account with only 100 USDC (admin is the mint authority)
    let subscriber_usdc = harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, 100);

    // Create the real Token-2022 ATA before attempting to wrap.
    let subscriber_wrapped =
        harness.create_token_2022_ata(&subscriber, &subscriber_pubkey, &wrapped_usdc_mint);

    // Try to wrap more than available
    let error = send_wrap(
        &mut harness,
        &subscriber,
        &spl_usdc_mint,
        &wrapped_usdc_mint,
        &subscriber_usdc,
        &subscriber_wrapped,
        &subscriber_pubkey,
        &wrapping_vault,
        1_000_000, // 1 USDC but only have 100 lamports
    )
    .expect_err("wrap with insufficient balance should fail");

    let err_string = format!("{:?}", error.err);
    assert!(
        err_string.contains("Custom(") || err_string.contains("InsufficientFunds"),
        "expected insufficient funds error, got {:?}",
        error.err,
    );
}
