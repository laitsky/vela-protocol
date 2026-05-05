#![allow(clippy::too_many_arguments)]

#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::{to_address, TestHarness};
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{
        PlanStatus, PricingTier, UsagePlan, VelaPlan, ACCOUNT_RESERVED_BYTES,
        CURRENT_ACCOUNT_VERSION,
    },
};

/// Build V1 flat plan bytes manually (no version/_reserved fields).
fn build_v1_flat_plan_bytes(
    merchant: &Pubkey,
    plan_id: u64,
    amount: u64,
    frequency: u64,
    trial_period: u64,
    max_pulls: u64,
    status: &PlanStatus,
    credential_mint: &Pubkey,
    bump: u8,
) -> Vec<u8> {
    let mut data = Vec::new();
    anchor_lang::AnchorSerialize::serialize(merchant, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&plan_id, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&amount, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&frequency, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&trial_period, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&max_pulls, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(status, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(credential_mint, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&bump, &mut data).unwrap();
    data
}

/// Build V1 usage plan bytes manually (no version/_reserved fields).
fn build_v1_usage_plan_bytes(
    merchant: &Pubkey,
    plan_id: u64,
    unit_name: &[u8; 32],
    tiers: &[PricingTier; 5],
    tier_count: u8,
    max_charge_per_period: u64,
    settlement_frequency: u64,
    credential_mint: &Pubkey,
    status: &PlanStatus,
    bump: u8,
) -> Vec<u8> {
    let mut data = Vec::new();
    anchor_lang::AnchorSerialize::serialize(merchant, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&plan_id, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(unit_name, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(tiers, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&tier_count, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&max_charge_per_period, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&settlement_frequency, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(credential_mint, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(status, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&bump, &mut data).unwrap();
    data
}

fn inject_legacy_flat_plan(
    harness: &mut TestHarness,
    plan_key: &Pubkey,
    merchant: &Pubkey,
    plan_id: u64,
    amount: u64,
    frequency: u64,
    trial_period: u64,
    max_pulls: u64,
    credential_mint: &Pubkey,
    bump: u8,
) {
    use anchor_lang::Discriminator;
    let body = build_v1_flat_plan_bytes(
        merchant,
        plan_id,
        amount,
        frequency,
        trial_period,
        max_pulls,
        &PlanStatus::Active,
        credential_mint,
        bump,
    );
    let mut full_data = Vec::new();
    full_data.extend_from_slice(VelaPlan::DISCRIMINATOR);
    full_data.extend_from_slice(&body);

    harness
        .svm
        .set_account(
            to_address(*plan_key),
            solana_account::Account {
                lamports: 1_500_000,
                data: full_data,
                owner: harness.program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("legacy plan account should be created");
}

fn inject_legacy_flat_plan_with_trailing_byte(
    harness: &mut TestHarness,
    plan_key: &Pubkey,
    merchant: &Pubkey,
    plan_id: u64,
    credential_mint: &Pubkey,
    bump: u8,
) {
    use anchor_lang::Discriminator;
    let body = build_v1_flat_plan_bytes(
        merchant,
        plan_id,
        25_000_000,
        MIN_FREQUENCY_SECONDS,
        0,
        4,
        &PlanStatus::Active,
        credential_mint,
        bump,
    );
    let mut full_data = Vec::new();
    full_data.extend_from_slice(VelaPlan::DISCRIMINATOR);
    full_data.extend_from_slice(&body);
    full_data.push(1);

    harness
        .svm
        .set_account(
            to_address(*plan_key),
            solana_account::Account {
                lamports: 1_500_000,
                data: full_data,
                owner: harness.program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("malformed legacy plan account should be created");
}

fn inject_legacy_usage_plan(
    harness: &mut TestHarness,
    plan_key: &Pubkey,
    merchant: &Pubkey,
    plan_id: u64,
    unit_name: &[u8; 32],
    tiers: &[PricingTier; 5],
    tier_count: u8,
    max_charge_per_period: u64,
    settlement_frequency: u64,
    credential_mint: &Pubkey,
    bump: u8,
) {
    use anchor_lang::Discriminator;
    let body = build_v1_usage_plan_bytes(
        merchant,
        plan_id,
        unit_name,
        tiers,
        tier_count,
        max_charge_per_period,
        settlement_frequency,
        credential_mint,
        &PlanStatus::Active,
        bump,
    );
    let mut full_data = Vec::new();
    full_data.extend_from_slice(UsagePlan::DISCRIMINATOR);
    full_data.extend_from_slice(&body);

    harness
        .svm
        .set_account(
            to_address(*plan_key),
            solana_account::Account {
                lamports: 1_500_000,
                data: full_data,
                owner: harness.program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("legacy usage plan account should be created");
}

fn inject_legacy_usage_plan_with_trailing_byte(
    harness: &mut TestHarness,
    plan_key: &Pubkey,
    merchant: &Pubkey,
    plan_id: u64,
    credential_mint: &Pubkey,
    bump: u8,
) {
    use anchor_lang::Discriminator;
    let tiers = [PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }; 5];
    let body = build_v1_usage_plan_bytes(
        merchant,
        plan_id,
        &[0u8; 32],
        &tiers,
        1,
        50_000_000,
        3600,
        credential_mint,
        &PlanStatus::Active,
        bump,
    );
    let mut full_data = Vec::new();
    full_data.extend_from_slice(UsagePlan::DISCRIMINATOR);
    full_data.extend_from_slice(&body);
    full_data.push(1);

    harness
        .svm
        .set_account(
            to_address(*plan_key),
            solana_account::Account {
                lamports: 1_500_000,
                data: full_data,
                owner: harness.program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("malformed legacy usage plan account should be created");
}

/// Test: Fresh V2 flat plan created via create_plan has version=CURRENT_ACCOUNT_VERSION.
#[test]
fn test_fresh_flat_plan_is_v2() {
    let mut harness = TestHarness::new();

    let amount = 25_000_000;
    let frequency = MIN_FREQUENCY_SECONDS;
    let trial_period = 86_400u64;
    let max_pulls = 12u64;
    let addresses = harness.derive_plan_addresses(0);

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(amount, frequency, trial_period, max_pulls, 0)
        .expect("create_plan should succeed");

    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    assert_eq!(
        plan.version, CURRENT_ACCOUNT_VERSION,
        "Fresh plan must be V2"
    );
    assert_eq!(
        plan._reserved,
        [0u8; vela_protocol::state::PLAN_RESERVED_BYTES],
        "Reserved must be zero-filled"
    );

    // Business fields preserved
    assert_eq!(plan.amount, amount);
    assert_eq!(plan.frequency, frequency);
    assert_eq!(plan.trial_period, trial_period);
    assert_eq!(plan.max_pulls, max_pulls);
    assert_eq!(plan.merchant, harness.merchant_pubkey());
    assert_eq!(plan.plan_id, 0);
    assert!(matches!(plan.status, PlanStatus::Active));
}

/// Test: Fresh V2 usage plan created via create_usage_plan has version=CURRENT_ACCOUNT_VERSION.
#[test]
fn test_fresh_usage_plan_is_v2() {
    let mut harness = TestHarness::new();

    let plan_id = 99u64;
    let unit_name = *b"api_calls\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let max_charge = 50_000_000u64;
    let settlement_freq = 3600u64;

    let addresses = harness.derive_usage_plan_addresses(plan_id);

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_usage_plan(plan_id, unit_name, tiers, max_charge, settlement_freq)
        .expect("create_usage_plan should succeed");

    let plan: UsagePlan = harness.fetch_anchor_account(&addresses.usage_plan);
    assert_eq!(
        plan.version, CURRENT_ACCOUNT_VERSION,
        "Fresh usage plan must be V2"
    );
    assert_eq!(plan._reserved, [0u8; ACCOUNT_RESERVED_BYTES - 32]);
    assert_eq!(plan.merchant, harness.merchant_pubkey());
    assert_eq!(plan.plan_id, plan_id);
    assert_eq!(plan.max_charge_per_period, max_charge);
    assert_eq!(plan.settlement_frequency, settlement_freq);
}

/// Test: migrate_plan upgrades a V1 flat plan to V2 in place.
/// V1 plan data (merchant, amount, frequency, etc.) must be preserved.
#[test]
fn test_migrate_flat_plan_preserves_business_fields() {
    let mut harness = TestHarness::new();

    let merchant = harness.merchant_pubkey();
    let addresses = harness.derive_plan_addresses(0);

    // Inject a V1 flat plan (no version/reserved fields)
    let dummy_credential = Pubkey::new_unique();
    harness.inject_token_2022_mint(&dummy_credential, &Pubkey::default(), 0);

    let plan_id_bytes = 0u64.to_le_bytes();
    let (_, plan_bump) = Pubkey::find_program_address(
        &[
            VelaPlan::SEED_PREFIX,
            merchant.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &vela_protocol::ID,
    );

    let amount = 25_000_000u64;
    let frequency = MIN_FREQUENCY_SECONDS;
    let trial_period = 86_400u64;
    let max_pulls = 12u64;

    inject_legacy_flat_plan(
        &mut harness,
        &addresses.plan,
        &merchant,
        0,
        amount,
        frequency,
        trial_period,
        max_pulls,
        &dummy_credential,
        plan_bump,
    );

    // Verify V1 plan exists and has legacy size
    let plan_data = harness.fetch_account_data(&addresses.plan);
    let v1_expected_size = 8 + 32 + 8 + 8 + 8 + 8 + 8 + 1 + 32 + 1; // 114 (discriminator + V1 fields)
    assert_eq!(
        plan_data.len(),
        v1_expected_size,
        "V1 plan should have legacy size"
    );

    // Migrate the plan
    harness
        .send_migrate_plan(&addresses.plan)
        .expect("migrate_plan should succeed");

    // Verify V2 plan data
    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    assert_eq!(
        plan.version, CURRENT_ACCOUNT_VERSION,
        "Migrated plan must be V2"
    );
    assert_eq!(
        plan._reserved,
        [0u8; vela_protocol::state::PLAN_RESERVED_BYTES]
    );

    // All business fields preserved
    assert_eq!(plan.merchant, merchant);
    assert_eq!(plan.plan_id, 0);
    assert_eq!(plan.amount, amount);
    assert_eq!(plan.frequency, frequency);
    assert_eq!(plan.trial_period, trial_period);
    assert_eq!(plan.max_pulls, max_pulls);
    assert!(matches!(plan.status, PlanStatus::Active));
    assert_eq!(plan.credential_mint, dummy_credential);
    assert_eq!(plan.bump, plan_bump);
}

/// Test: migrate_plan upgrades a V1 usage plan to V2 in place.
#[test]
fn test_migrate_usage_plan_preserves_business_fields() {
    let mut harness = TestHarness::new();

    let merchant = harness.merchant_pubkey();
    let plan_id = 42u64;
    let addresses = harness.derive_usage_plan_addresses(plan_id);

    // Inject a V1 usage plan
    let dummy_credential = Pubkey::new_unique();
    harness.inject_token_2022_mint(&dummy_credential, &Pubkey::default(), 0);

    let plan_id_bytes = plan_id.to_le_bytes();
    let (_, usage_bump) = Pubkey::find_program_address(
        &[
            UsagePlan::SEED_PREFIX,
            merchant.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &vela_protocol::ID,
    );

    let tiers = [PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }; 5];
    let max_charge = 50_000_000u64;
    let settlement_freq = 3600u64;

    inject_legacy_usage_plan(
        &mut harness,
        &addresses.usage_plan,
        &merchant,
        plan_id,
        &[0u8; 32],
        &tiers,
        1,
        max_charge,
        settlement_freq,
        &dummy_credential,
        usage_bump,
    );

    // Migrate the plan
    harness
        .send_migrate_plan(&addresses.usage_plan)
        .expect("migrate_plan should succeed for usage plans");

    let plan: UsagePlan = harness.fetch_anchor_account(&addresses.usage_plan);
    assert_eq!(
        plan.version, CURRENT_ACCOUNT_VERSION,
        "Migrated usage plan must be V2"
    );
    assert_eq!(plan._reserved, [0u8; ACCOUNT_RESERVED_BYTES - 32]);

    // All business fields preserved
    assert_eq!(plan.merchant, merchant);
    assert_eq!(plan.plan_id, plan_id);
    assert_eq!(plan.tier_count, 1);
    assert_eq!(plan.max_charge_per_period, max_charge);
    assert_eq!(plan.settlement_frequency, settlement_freq);
    assert_eq!(plan.credential_mint, dummy_credential);
    assert_eq!(plan.bump, usage_bump);
}

#[test]
fn test_migrate_flat_plan_rejects_nonzero_legacy_trailing_bytes() {
    let mut harness = TestHarness::new();
    let merchant = harness.merchant_pubkey();
    let addresses = harness.derive_plan_addresses(11);
    let dummy_credential = Pubkey::new_unique();
    harness.inject_token_2022_mint(&dummy_credential, &Pubkey::default(), 0);

    let plan_id_bytes = 11u64.to_le_bytes();
    let (_, plan_bump) = Pubkey::find_program_address(
        &[
            VelaPlan::SEED_PREFIX,
            merchant.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &vela_protocol::ID,
    );

    inject_legacy_flat_plan_with_trailing_byte(
        &mut harness,
        &addresses.plan,
        &merchant,
        11,
        &dummy_credential,
        plan_bump,
    );

    harness
        .send_migrate_plan(&addresses.plan)
        .expect_err("nonzero legacy trailing bytes must be rejected");
}

#[test]
fn test_migrate_usage_plan_rejects_nonzero_legacy_trailing_bytes() {
    let mut harness = TestHarness::new();
    let merchant = harness.merchant_pubkey();
    let plan_id = 43u64;
    let addresses = harness.derive_usage_plan_addresses(plan_id);
    let dummy_credential = Pubkey::new_unique();
    harness.inject_token_2022_mint(&dummy_credential, &Pubkey::default(), 0);

    let plan_id_bytes = plan_id.to_le_bytes();
    let (_, usage_bump) = Pubkey::find_program_address(
        &[
            UsagePlan::SEED_PREFIX,
            merchant.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &vela_protocol::ID,
    );

    inject_legacy_usage_plan_with_trailing_byte(
        &mut harness,
        &addresses.usage_plan,
        &merchant,
        plan_id,
        &dummy_credential,
        usage_bump,
    );

    harness
        .send_migrate_plan(&addresses.usage_plan)
        .expect_err("nonzero legacy usage trailing bytes must be rejected");
}

/// Test: migrate_plan on already-V2 plan is a no-op (idempotent).
#[test]
fn test_migrate_plan_idempotent_on_v2() {
    let mut harness = TestHarness::new();

    let addresses = harness.derive_plan_addresses(0);
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");

    let before: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    assert_eq!(before.version, CURRENT_ACCOUNT_VERSION);

    // Migrating an already-V2 plan should succeed without errors
    harness
        .send_migrate_plan(&addresses.plan)
        .expect("migrate_plan should be idempotent on V2 plans");

    let after: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    assert_eq!(after.version, CURRENT_ACCOUNT_VERSION);
    assert_eq!(after.amount, before.amount);
    assert_eq!(after.frequency, before.frequency);
    assert_eq!(after.bump, before.bump);
}
