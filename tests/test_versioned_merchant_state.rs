#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::{
    prelude::{borsh, Pubkey},
    AnchorDeserialize, AnchorSerialize, Discriminator,
};
use helpers::{to_address, TestHarness};
use solana_account::Account;
use vela_protocol::state::{MerchantState, PricingTier};

#[derive(AnchorSerialize, AnchorDeserialize)]
struct LegacyMerchantState {
    merchant: Pubkey,
    plan_count: u64,
    bump: u8,
}

fn merchant_state_address(merchant: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[vela_protocol::state::MerchantState::SEED_PREFIX, merchant.as_ref()],
        &vela_protocol::ID,
    )
}

fn write_legacy_merchant_state(
    harness: &mut TestHarness,
    merchant_state: Pubkey,
    merchant: Pubkey,
    plan_count: u64,
    bump: u8,
) {
    let legacy = LegacyMerchantState {
        merchant,
        plan_count,
        bump,
    };
    let mut data = MerchantState::DISCRIMINATOR.to_vec();
    legacy
        .serialize(&mut data)
        .expect("legacy merchant state should serialize");

    harness
        .svm
        .set_account(
            to_address(merchant_state),
            Account {
                lamports: 10_000_000,
                data,
                owner: to_address(vela_protocol::ID),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("legacy merchant state should write");
}

#[test]
fn test_create_plan_upgrades_legacy_merchant_state() {
    let mut harness = TestHarness::new();
    let merchant = harness.merchant_pubkey();
    let (merchant_state, bump) = merchant_state_address(&merchant);

    write_legacy_merchant_state(&mut harness, merchant_state, merchant, 0, bump);

    harness
        .send_create_plan(25_000_000, vela_protocol::constants::MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should upgrade a legacy merchant state");

    let upgraded: MerchantState = harness.fetch_anchor_account(&merchant_state);
    assert_eq!(upgraded.merchant, merchant);
    assert_eq!(upgraded.plan_count, 1);
    assert_eq!(upgraded.credential_mint, Pubkey::default());
    assert_eq!(upgraded.mandate_counter, 0);
    assert_eq!(upgraded.version, 1);
    assert_eq!(upgraded._reserved, [0u8; 64]);
}

#[test]
fn test_create_usage_plan_initializes_versioned_merchant_state() {
    let mut harness = TestHarness::new();
    let merchant = harness.merchant_pubkey();
    let (merchant_state, _) = merchant_state_address(&merchant);

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
    assert_eq!(state.credential_mint, Pubkey::default());
    assert_eq!(state.mandate_counter, 0);
    assert_eq!(state.version, 1);
    assert_eq!(state._reserved, [0u8; 64]);
}
