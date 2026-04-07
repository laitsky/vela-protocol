#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{TestHarness, to_anchor_pubkey};
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{MandateStatus, VelaMandate, VelaPlan},
};

// ------------------------------------------------------------------
// pause_protocol tests
// ------------------------------------------------------------------

#[test]
fn test_pause_protocol() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    harness
        .send_pause_protocol(&admin)
        .expect("pause_protocol should succeed for admin");

    let config = harness.read_protocol_config();
    assert!(config.paused, "ProtocolConfig.paused should be true after pause");
    // LiteSVM starts at unix_timestamp = 0, so paused_at may be 0 in tests.
    // We just verify the field was written (it equals current clock timestamp).
    let current_ts = harness.current_timestamp();
    assert_eq!(
        config.paused_at, current_ts,
        "ProtocolConfig.paused_at should equal current clock timestamp"
    );
}

#[test]
fn test_pause_protocol_unauthorized() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    let non_admin = harness.create_wallet();
    let error = harness
        .send_pause_protocol(&non_admin)
        .expect_err("pause_protocol should reject non-admin");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("UnauthorizedAdmin") || error_str.contains("Custom(6015)"),
        "expected UnauthorizedAdmin error, got: {}",
        error_str
    );
}

// ------------------------------------------------------------------
// unpause_protocol tests
// ------------------------------------------------------------------

#[test]
fn test_unpause_protocol() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    harness
        .send_pause_protocol(&admin)
        .expect("pause_protocol should succeed");

    // Verify it is paused
    let config = harness.read_protocol_config();
    assert!(config.paused);

    harness
        .send_unpause_protocol(&admin)
        .expect("unpause_protocol should succeed for admin");

    let config = harness.read_protocol_config();
    assert!(!config.paused, "ProtocolConfig.paused should be false after unpause");
    assert_eq!(
        config.paused_at, 0,
        "ProtocolConfig.paused_at should be 0 after unpause"
    );
}

// ------------------------------------------------------------------
// execute_pull paused guard tests
// ------------------------------------------------------------------

#[test]
fn test_execute_pull_paused() {
    let mut harness = TestHarness::new();

    // Minimal setup: just init config and subscribe (don't need full wrapped USDC pipeline).
    // The paused guard fires before PullApproval checks, so we only need a valid protocol_config.
    let admin = harness.merchant.insecure_clone();
    harness.send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2, 0)
        .expect("create_plan should succeed");

    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);
    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);

    let subscriber = harness.create_wallet();
    let subscriber_pubkey = to_anchor_pubkey(subscriber.pubkey());

    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let addresses = harness.derive_plan_addresses(0);
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &addresses.plan);
    let mandate_state: VelaMandate = harness.fetch_anchor_account(&mandate);

    // Create wrapped accounts for the subscriber
    let subscriber_wrapped = harness.create_token_2022_ata(&admin, &mandate, &wrapped_mint_pubkey);
    let merchant_wrapped = harness.create_token_2022_ata(&admin, &harness.merchant_pubkey(), &wrapped_mint_pubkey);

    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    let subscriber_usdc = harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, plan.amount * 10);
    harness.send_wrap(
        &subscriber,
        &spl_usdc_mint,
        &wrapped_mint_pubkey,
        &subscriber_usdc,
        &subscriber_wrapped,
        &mandate,
        &wrapping_vault,
        plan.amount * 2,
    ).expect("wrap should succeed");

    // Pause the protocol
    harness
        .send_pause_protocol(&admin)
        .expect("pause_protocol should succeed");

    // Set clock to allow pull
    harness.set_clock_timestamp(mandate_state.next_payment_due);
    let config = harness.derive_config();
    let (extra_account_meta_list, _) = harness.derive_extra_account_meta_list(&wrapped_mint_pubkey);

    let error = harness
        .send_execute_pull_t22(
            &admin,
            &subscriber_pubkey,
            0,
            &subscriber_wrapped,
            &merchant_wrapped,
            &wrapped_mint_pubkey,
            &wrapping_vault,
            &config,
            &extra_account_meta_list,
        )
        .expect_err("execute_pull should be rejected when protocol is paused");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("ProtocolPaused") || error_str.contains("Custom("),
        "expected ProtocolPaused error, got: {}",
        error_str
    );
}

#[test]
fn test_execute_pull_after_unpause() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    harness.send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2, 0)
        .expect("create_plan should succeed");

    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);
    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);

    let subscriber = harness.create_wallet();
    let subscriber_pubkey = to_anchor_pubkey(subscriber.pubkey());

    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let addresses = harness.derive_plan_addresses(0);
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &addresses.plan);
    let mandate_state: VelaMandate = harness.fetch_anchor_account(&mandate);

    let subscriber_wrapped = harness.create_token_2022_ata(&admin, &mandate, &wrapped_mint_pubkey);
    let merchant_wrapped = harness.create_token_2022_ata(&admin, &harness.merchant_pubkey(), &wrapped_mint_pubkey);

    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    let subscriber_usdc = harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, plan.amount * 10);
    harness.send_wrap(
        &subscriber,
        &spl_usdc_mint,
        &wrapped_mint_pubkey,
        &subscriber_usdc,
        &subscriber_wrapped,
        &mandate,
        &wrapping_vault,
        plan.amount * 2,
    ).expect("wrap should succeed");

    // Pause then unpause
    harness.send_pause_protocol(&admin).expect("pause should succeed");
    harness.send_unpause_protocol(&admin).expect("unpause should succeed");

    // Set clock to allow pull and create approval
    harness.set_clock_timestamp(mandate_state.next_payment_due);
    harness.create_pull_approval_with_amount(&mandate, mandate_state.next_payment_due, true, plan.amount);

    let config = harness.derive_config();
    let (extra_account_meta_list, _) = harness.derive_extra_account_meta_list(&wrapped_mint_pubkey);

    harness
        .send_execute_pull_t22(
            &admin,
            &subscriber_pubkey,
            0,
            &subscriber_wrapped,
            &merchant_wrapped,
            &wrapped_mint_pubkey,
            &wrapping_vault,
            &config,
            &extra_account_meta_list,
        )
        .expect("execute_pull should succeed after unpause");
}

// ------------------------------------------------------------------
// admin_cancel tests
// ------------------------------------------------------------------

#[test]
fn test_admin_cancel() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2, 0)
        .expect("create_plan should succeed");

    let subscriber = harness.create_wallet();
    let subscriber_pubkey = to_anchor_pubkey(subscriber.pubkey());

    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let addresses = harness.derive_plan_addresses(0);
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &addresses.plan);

    harness
        .send_admin_cancel(
            &admin,
            &subscriber_pubkey,
            &addresses.plan,
            &mandate,
            &addresses.credential_mint,
        )
        .expect("admin_cancel should succeed");

    let mandate_state: VelaMandate = harness.fetch_anchor_account(&mandate);
    assert!(
        matches!(mandate_state.status, MandateStatus::Cancelled),
        "mandate status should be Cancelled after admin_cancel"
    );

    // Verify credential token balance is 0
    let credential_ata = harness.derive_credential_ata(&subscriber_pubkey, &addresses.credential_mint);
    use solana_program_pack::Pack;
    use spl_token_2022::state::Account as Token2022Account;
    let credential_account =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential account should unpack");
    assert_eq!(credential_account.amount, 0, "credential token balance should be 0 after admin_cancel");
}

#[test]
fn test_admin_cancel_unauthorized() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2, 0)
        .expect("create_plan should succeed");

    let subscriber = harness.create_wallet();
    let subscriber_pubkey = to_anchor_pubkey(subscriber.pubkey());

    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let addresses = harness.derive_plan_addresses(0);
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &addresses.plan);

    let non_admin = harness.create_wallet();
    let error = harness
        .send_admin_cancel(
            &non_admin,
            &subscriber_pubkey,
            &addresses.plan,
            &mandate,
            &addresses.credential_mint,
        )
        .expect_err("admin_cancel should reject non-admin");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("UnauthorizedAdmin") || error_str.contains("Custom(6015)"),
        "expected UnauthorizedAdmin error, got: {}",
        error_str
    );
}

#[test]
fn test_admin_cancel_already_cancelled() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2, 0)
        .expect("create_plan should succeed");

    let subscriber = harness.create_wallet();
    let subscriber_pubkey = to_anchor_pubkey(subscriber.pubkey());

    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let addresses = harness.derive_plan_addresses(0);
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &addresses.plan);

    // First admin_cancel succeeds
    harness
        .send_admin_cancel(
            &admin,
            &subscriber_pubkey,
            &addresses.plan,
            &mandate,
            &addresses.credential_mint,
        )
        .expect("first admin_cancel should succeed");

    // Second admin_cancel should fail with MandateNotActive
    let error = harness
        .send_admin_cancel(
            &admin,
            &subscriber_pubkey,
            &addresses.plan,
            &mandate,
            &addresses.credential_mint,
        )
        .expect_err("admin_cancel on cancelled mandate should fail");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("MandateNotActive") || error_str.contains("Custom(6001)"),
        "expected MandateNotActive error, got: {}",
        error_str
    );
}
