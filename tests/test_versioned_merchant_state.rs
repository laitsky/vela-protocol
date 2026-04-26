#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use vela_protocol::state::{MerchantState, PricingTier};

fn merchant_state_address(merchant: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            vela_protocol::state::MerchantState::SEED_PREFIX,
            merchant.as_ref(),
        ],
        &vela_protocol::ID,
    )
}

#[test]
fn test_create_plan_upgrades_legacy_merchant_state() {
    let mut harness = TestHarness::new();
    let merchant = harness.merchant_pubkey();
    let (merchant_state, _bump) = merchant_state_address(&merchant);

    // Post D-03: merchant credential bootstrap is required before create_plan.
    // Init credential first, then verify create_plan succeeds and merchant state
    // has the credential mint set (not Pubkey::default).
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");

    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();

    harness
        .send_create_plan(
            25_000_000,
            vela_protocol::constants::MIN_FREQUENCY_SECONDS,
            0,
            4,
            0,
        )
        .expect("create_plan should succeed after merchant credential bootstrap");

    let upgraded: MerchantState = harness.fetch_anchor_account(&merchant_state);
    assert_eq!(upgraded.merchant, merchant);
    assert_eq!(upgraded.plan_count, 1);
    assert_eq!(upgraded.credential_mint, merchant_credential_mint);
    assert_eq!(upgraded.mandate_counter, 0);
    assert_eq!(upgraded.stream_mandate_counter, 0);
    assert_eq!(upgraded.version, 1);
    assert_eq!(upgraded._reserved, [0u8; 56]);
}

#[test]
fn test_create_usage_plan_initializes_versioned_merchant_state() {
    let mut harness = TestHarness::new();
    let merchant = harness.merchant_pubkey();
    let (merchant_state, _) = merchant_state_address(&merchant);

    // Post D-03: merchant credential bootstrap is required before create_usage_plan.
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");

    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();

    harness
        .send_create_usage_plan(
            9,
            *b"api_calls\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            vec![PricingTier {
                up_to: 100,
                rate_per_unit: 5,
                _padding: 0,
            }],
            100_000,
            3_600,
        )
        .expect("create_usage_plan should initialize merchant state");

    let state: MerchantState = harness.fetch_anchor_account(&merchant_state);
    assert_eq!(state.merchant, merchant);
    assert_eq!(state.plan_count, 0);
    assert_eq!(state.credential_mint, merchant_credential_mint);
    assert_eq!(state.mandate_counter, 0);
    assert_eq!(state.stream_mandate_counter, 0);
    assert_eq!(state.version, 1);
    assert_eq!(state._reserved, [0u8; 56]);
}
