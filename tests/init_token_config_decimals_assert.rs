#[path = "helpers/mod.rs"]
mod helpers;

use helpers::TestHarness;
use vela_protocol::state::BillingRail;

fn setup_fixture() -> (
    TestHarness,
    solana_keypair::Keypair,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
) {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let protocol_config = harness.init_protocol_config(&admin);
    let mint = harness.create_spl_mint(&admin, 6);

    (harness, admin, protocol_config, mint)
}

#[test]
fn init_token_config_accepts_matching_mint_decimals() {
    let (mut harness, admin, _protocol_config, mint) = setup_fixture();

    let token_config = harness.init_token_config(&admin, &mint, BillingRail::TransferHook, 6);
    let account: vela_protocol::state::TokenConfig = harness.fetch_anchor_account(&token_config);

    assert_eq!(account.mint, mint);
    assert_eq!(account.decimals, 6);
}

#[test]
fn init_token_config_rejects_mismatched_mint_decimals() {
    let (mut harness, admin, protocol_config, mint) = setup_fixture();

    let error = harness
        .send_init_token_config(
            &admin,
            &protocol_config,
            &mint,
            BillingRail::TransferHook,
            9,
        )
        .expect_err("mismatched decimals should fail");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("TokenConfigDecimalsMismatch") || error_str.contains("Custom(12713)"),
        "expected TokenConfigDecimalsMismatch error, got {:?}",
        error.err,
    );
}
