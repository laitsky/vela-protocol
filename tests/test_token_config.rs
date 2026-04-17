#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_signer::Signer;
use vela_protocol::state::{BillingRail, TokenConfig, CURRENT_ACCOUNT_VERSION};

fn setup_token_config_fixture() -> (TestHarness, solana_keypair::Keypair, Pubkey, Pubkey) {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let protocol_config = harness.init_protocol_config(&admin);
    let mint = harness.create_spl_mint(&admin, 6);

    (harness, admin, protocol_config, mint)
}

#[test]
fn test_init_token_config_creates_pda() {
    let (mut harness, admin, protocol_config, mint) = setup_token_config_fixture();

    let token_config = harness.init_token_config(&admin, &mint, BillingRail::TransferHook, 6);
    let account: TokenConfig = harness.fetch_anchor_account(&token_config);

    assert_eq!(token_config, harness.derive_token_config_address(&mint));
    assert_eq!(account.mint, mint);
    assert_eq!(account.billing_rail, BillingRail::TransferHook);
    assert_eq!(account.decimals, 6);
    assert!(account.enabled);
    assert_eq!(account.oracle_reference, Pubkey::default());
    assert_eq!(account.version, CURRENT_ACCOUNT_VERSION);
    assert_eq!(
        account.admin,
        Pubkey::new_from_array(admin.pubkey().to_bytes())
    );
    assert_eq!(
        account.token_program,
        helpers::to_anchor_pubkey(harness.fetch_account_owner(&mint))
    );
    assert_eq!(protocol_config, harness.derive_config());
}

#[test]
fn test_init_token_config_non_admin_fails() {
    let (mut harness, admin, protocol_config, mint) = setup_token_config_fixture();
    let non_admin = harness.create_wallet();

    let error = harness
        .send_init_token_config(
            &non_admin,
            &protocol_config,
            &mint,
            BillingRail::TransferHook,
            6,
        )
        .expect_err("non-admin init_token_config should fail");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("UnauthorizedAdmin") || error_str.contains("Custom(6015)"),
        "expected UnauthorizedAdmin error, got {:?}",
        error.err,
    );

    let token_config = harness.derive_token_config_address(&mint);
    assert!(
        harness
            .svm
            .get_account(&helpers::to_address(token_config))
            .is_none(),
        "token config PDA must not be created for non-admin attempts"
    );
    assert_eq!(admin.pubkey(), harness.merchant.pubkey());
}

#[test]
fn test_init_token_config_duplicate_mint_fails() {
    let (mut harness, admin, protocol_config, mint) = setup_token_config_fixture();

    harness.init_token_config(&admin, &mint, BillingRail::TransferHook, 6);

    let result = harness.send_init_token_config(
        &admin,
        &protocol_config,
        &mint,
        BillingRail::TransferHook,
        6,
    );
    assert!(result.is_err(), "duplicate token config init must fail");
}

#[test]
fn test_update_token_config_toggles_enabled() {
    let (mut harness, admin, protocol_config, mint) = setup_token_config_fixture();
    let token_config = harness.init_token_config(&admin, &mint, BillingRail::TransferHook, 6);

    harness
        .update_token_config(&admin, &protocol_config, &token_config, Some(false), None)
        .expect("admin should be able to disable token config");
    let disabled: TokenConfig = harness.fetch_anchor_account(&token_config);
    assert!(!disabled.enabled);

    harness
        .update_token_config(&admin, &protocol_config, &token_config, Some(true), None)
        .expect("admin should be able to re-enable token config");
    let reenabled: TokenConfig = harness.fetch_anchor_account(&token_config);
    assert!(reenabled.enabled);
}

#[test]
fn test_update_token_config_updates_oracle() {
    let (mut harness, admin, protocol_config, mint) = setup_token_config_fixture();
    let token_config = harness.init_token_config(&admin, &mint, BillingRail::TransferHook, 6);
    let oracle_reference = Pubkey::new_unique();

    harness
        .update_token_config(
            &admin,
            &protocol_config,
            &token_config,
            None,
            Some(oracle_reference),
        )
        .expect("admin should be able to update oracle reference");

    let account: TokenConfig = harness.fetch_anchor_account(&token_config);
    assert_eq!(account.oracle_reference, oracle_reference);
    assert!(account.enabled);
}

#[test]
fn test_update_token_config_non_admin_fails() {
    let (mut harness, admin, protocol_config, mint) = setup_token_config_fixture();
    let token_config = harness.init_token_config(&admin, &mint, BillingRail::TransferHook, 6);
    let non_admin = harness.create_wallet();

    let error = harness
        .update_token_config(
            &non_admin,
            &protocol_config,
            &token_config,
            Some(false),
            None,
        )
        .expect_err("non-admin update_token_config should fail");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("UnauthorizedAdmin") || error_str.contains("Custom(6015)"),
        "expected UnauthorizedAdmin error, got {:?}",
        error.err,
    );

    let account: TokenConfig = harness.fetch_anchor_account(&token_config);
    assert!(
        account.enabled,
        "non-admin update must not mutate token config"
    );
}

#[test]
fn test_update_token_config_no_fields_fails() {
    let (mut harness, admin, protocol_config, mint) = setup_token_config_fixture();
    let token_config = harness.init_token_config(&admin, &mint, BillingRail::TransferHook, 6);

    let result = harness.update_token_config(&admin, &protocol_config, &token_config, None, None);
    assert!(result.is_err(), "empty update_token_config must fail");
}
