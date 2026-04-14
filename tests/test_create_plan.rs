#[path = "helpers/mod.rs"]
mod helpers;

use helpers::{token_2022_address, TestHarness};
use anchor_lang::prelude::Pubkey;
use spl_token_2022::{
    extension::{
        metadata_pointer::MetadataPointer,
        non_transferable::NonTransferable,
        BaseStateWithExtensions,
        StateWithExtensions,
    },
    state::Mint,
};
use vela_protocol::{
    constants::{CREDENTIAL_DECIMALS, MIN_FREQUENCY_SECONDS},
    state::{MerchantState, PlanStatus, VelaPlan},
};

#[test]
fn test_create_plan_initializes_plan_and_credential_mint() {
    let mut harness = TestHarness::new();
    let amount = 25_000_000;
    let frequency = MIN_FREQUENCY_SECONDS;
    let trial_period = 86_400;
    let max_pulls = 12;
    let addresses = harness.derive_plan_addresses(0);

    let meta = harness
        .send_create_plan(amount, frequency, trial_period, max_pulls, 0)
        .expect("create_plan should succeed");

    assert!(meta.compute_units_consumed > 0);

    let merchant_state: MerchantState = harness.fetch_anchor_account(&addresses.merchant_state);
    assert_eq!(merchant_state.merchant, harness.merchant_pubkey());
    assert_eq!(merchant_state.plan_count, 1);
    assert_eq!(merchant_state.credential_mint, Pubkey::default());
    assert_eq!(merchant_state.mandate_counter, 0);
    assert_eq!(merchant_state.version, 1);
    assert_eq!(merchant_state._reserved, [0u8; 64]);

    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    assert_eq!(plan.merchant, harness.merchant_pubkey());
    assert_eq!(plan.plan_id, 0);
    assert_eq!(plan.amount, amount);
    assert_eq!(plan.frequency, frequency);
    assert_eq!(plan.trial_period, trial_period);
    assert_eq!(plan.max_pulls, max_pulls);
    assert!(matches!(plan.status, PlanStatus::Active));
    assert_eq!(plan.credential_mint, addresses.credential_mint);

    assert_eq!(
        harness.fetch_account_owner(&addresses.credential_mint),
        token_2022_address()
    );
    let mint_data = harness.fetch_account_data(&addresses.credential_mint);
    let mint = StateWithExtensions::<Mint>::unpack(&mint_data)
        .expect("credential mint should unpack with Token-2022 extensions");
    assert_eq!(mint.base.decimals, CREDENTIAL_DECIMALS);
    assert!(mint.get_extension::<NonTransferable>().is_ok());
    assert!(mint.get_extension::<MetadataPointer>().is_ok());
}

#[test]
fn test_create_plan_rejects_low_frequency() {
    let mut harness = TestHarness::new();
    let error = harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS - 1, 0, 4, 0)
        .expect_err("create_plan should reject plans below the minimum cadence");

    assert!(
        format!("{:?}", error.err).contains("Custom(6005)"),
        "expected FrequencyTooLow custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_create_plan_rejects_zero_max_pulls() {
    let mut harness = TestHarness::new();
    let error = harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 0, 0)
        .expect_err("create_plan should reject zero max_pulls");

    assert!(
        format!("{:?}", error.err).contains("Custom(6010)"),
        "expected MaxPullsTooLow custom error, got {:?}",
        error.err,
    );
}

// --- Phase 40-08: Merchant-credential-aware plan creation ---

#[test]
fn test_create_plan_uses_merchant_credential_after_bootstrap() {
    let mut harness = TestHarness::new();

    // Bootstrap merchant credential first (40-07)
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");

    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();

    // Create plan after bootstrap -- should use merchant credential mint
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed after merchant bootstrap");

    let addresses = harness.derive_plan_addresses(0);
    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);

    // New plans should reference the merchant credential mint, not a plan-scoped mint
    assert_eq!(
        plan.credential_mint, merchant_credential_mint,
        "create_plan must use merchant credential mint after bootstrap (CRED-01)"
    );

    // The plan-scoped credential mint PDA should NOT be created as a Token-2022 mint
    let plan_scoped_mint_account = harness.svm.get_account(
        &solana_address::Address::from(addresses.credential_mint.to_bytes()),
    );
    // It should either not exist or not be owned by Token-2022
    if let Some(account) = plan_scoped_mint_account {
        assert_ne!(
            account.owner,
            spl_token_2022::id().to_bytes().into(),
            "plan-scoped credential mint should not be created after merchant bootstrap"
        );
    }
}

#[test]
fn test_create_usage_plan_uses_merchant_credential_after_bootstrap() {
    let mut harness = TestHarness::new();
    use vela_protocol::state::PricingTier;

    // Bootstrap merchant credential first (40-07)
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");

    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();

    let mut unit_name = [0u8; 32];
    unit_name[..9].copy_from_slice(b"api_calls");
    let tiers = vec![
        PricingTier {
            up_to: 1_000,
            rate_per_unit: 10_000,
            _padding: 0,
        },
        PricingTier {
            up_to: 0,
            rate_per_unit: 8_000,
            _padding: 0,
        },
    ];

    harness
        .send_create_usage_plan(0, unit_name, tiers, 75_000_000, MIN_FREQUENCY_SECONDS * 2)
        .expect("create_usage_plan should succeed after merchant bootstrap");

    let addresses = harness.derive_usage_plan_addresses(0);
    let plan: vela_protocol::state::UsagePlan = harness.fetch_anchor_account(&addresses.usage_plan);

    // New usage plans should reference the merchant credential mint
    assert_eq!(
        plan.credential_mint, merchant_credential_mint,
        "create_usage_plan must use merchant credential mint after bootstrap (CRED-01)"
    );
}
