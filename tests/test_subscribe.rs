#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::AccountSerialize;
use helpers::TestHarness;
use solana_account::Account;
use solana_address::Address;
use solana_program_pack::Pack;
use solana_signer::Signer;
use spl_token_2022::state::Account as Token2022Account;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{
        BillingType, MandateStatus, MerchantState, PlanStatus, PricingTier, UsagePlan, VelaMandate,
        VelaPlan, ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION,
    },
};

use anchor_lang::prelude::Pubkey;

fn setup_subscription_fixture() -> (
    TestHarness,
    solana_keypair::Keypair,
    vela_protocol::state::VelaPlan,
    Pubkey,
    Pubkey,
) {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let amount = 25_000_000;
    let max_pulls = 4;
    let frequency = MIN_FREQUENCY_SECONDS;
    let addresses = harness.derive_plan_addresses(0);

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");

    harness
        .send_create_plan(amount, frequency, 0, max_pulls, 0)
        .expect("create_plan should succeed");

    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    let credential_mint = plan.credential_mint;
    (harness, subscriber, plan, addresses.plan, credential_mint)
}

fn seed_legacy_flat_plan(harness: &mut TestHarness, plan_id: u64) -> (Pubkey, Pubkey) {
    let merchant = harness.merchant_pubkey();
    let plan_addresses = harness.derive_plan_addresses(plan_id);
    let (merchant_state, merchant_state_bump) = Pubkey::find_program_address(
        &[MerchantState::SEED_PREFIX, merchant.as_ref()],
        &vela_protocol::ID,
    );
    let (_, plan_bump) = Pubkey::find_program_address(
        &[
            VelaPlan::SEED_PREFIX,
            merchant.as_ref(),
            plan_id.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    );

    let merchant_state_record = MerchantState {
        merchant,
        plan_count: plan_id + 1,
        bump: merchant_state_bump,
        credential_mint: Pubkey::default(),
        mandate_counter: 0,
        stream_mandate_counter: 0,
        version: CURRENT_ACCOUNT_VERSION,
        _reserved: [0; ACCOUNT_RESERVED_BYTES - 8],
    };
    let mut merchant_state_data = Vec::new();
    merchant_state_record
        .try_serialize(&mut merchant_state_data)
        .expect("merchant state should serialize");
    harness
        .svm
        .set_account(
            Address::from(merchant_state.to_bytes()),
            Account {
                lamports: 1_000_000,
                data: merchant_state_data,
                owner: harness.program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("merchant_state should be seeded");

    let plan_record = VelaPlan {
        merchant,
        plan_id,
        amount: 25_000_000,
        frequency: MIN_FREQUENCY_SECONDS,
        trial_period: 0,
        max_pulls: 4,
        status: PlanStatus::Active,
        credential_mint: plan_addresses.credential_mint,
        billing_mint: Pubkey::default(),
        bump: plan_bump,
        version: CURRENT_ACCOUNT_VERSION,
        _reserved: [0; vela_protocol::state::PLAN_RESERVED_BYTES],
    };
    let mut plan_data = Vec::new();
    plan_record
        .try_serialize(&mut plan_data)
        .expect("plan should serialize");
    harness
        .svm
        .set_account(
            Address::from(plan_addresses.plan.to_bytes()),
            Account {
                lamports: 1_000_000,
                data: plan_data,
                owner: harness.program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("legacy plan should be seeded");

    harness.inject_token_2022_mint(&plan_addresses.credential_mint, &plan_addresses.plan, 0);

    (plan_addresses.plan, plan_addresses.credential_mint)
}

#[test]
fn test_subscribe_success() {
    let (mut harness, subscriber, plan, plan_address, credential_mint) =
        setup_subscription_fixture();
    let now = harness.current_timestamp();

    let meta = harness
        .send_subscribe(&subscriber, plan.plan_id)
        .expect("subscribe should succeed");

    assert!(meta.compute_units_consumed > 0);

    let mandate_address = harness.derive_mandate_address(
        &Pubkey::new_from_array(subscriber.pubkey().to_bytes()),
        &plan_address,
    );
    let mandate: VelaMandate = harness.fetch_anchor_account(&mandate_address);
    assert_eq!(
        mandate.subscriber,
        Pubkey::new_from_array(subscriber.pubkey().to_bytes())
    );
    assert_eq!(mandate.plan, plan_address);
    assert_eq!(mandate.merchant, harness.merchant_pubkey());
    assert_eq!(mandate.amount, plan.amount);
    assert_eq!(mandate.frequency, plan.frequency);
    assert_eq!(mandate.start_date, now);
    assert_eq!(mandate.max_pulls, plan.max_pulls);
    assert_eq!(mandate.pulls_executed, 0);
    assert_eq!(mandate.next_payment_due, now + plan.frequency as i64);
    assert!(matches!(mandate.status, MandateStatus::Active));

    // No delegate approval -- subscribe no longer calls approve() (D-12, D-14)
    // Credential token must be minted to subscriber
    let credential_ata = harness.derive_credential_ata(
        &Pubkey::new_from_array(subscriber.pubkey().to_bytes()),
        &credential_mint,
    );
    let credential_account =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential account should unpack");
    assert_eq!(
        credential_account.mint.to_string(),
        credential_mint.to_string()
    );
    assert_eq!(
        credential_account.owner.to_string(),
        Pubkey::new_from_array(subscriber.pubkey().to_bytes()).to_string()
    );
    assert_eq!(credential_account.amount, 1);
}

#[test]
fn test_subscribe_trial_period_extends_expiry_from_first_bill() {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let amount = 25_000_000;
    let max_pulls = 4;
    let frequency = MIN_FREQUENCY_SECONDS;
    let trial_period = MIN_FREQUENCY_SECONDS * 2;
    let addresses = harness.derive_plan_addresses(0);

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(amount, frequency, trial_period, max_pulls, 0)
        .expect("create_plan should succeed");

    let now = harness.current_timestamp();
    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);

    harness
        .send_subscribe(&subscriber, plan.plan_id)
        .expect("subscribe should succeed");

    let mandate_address = harness.derive_mandate_address(
        &Pubkey::new_from_array(subscriber.pubkey().to_bytes()),
        &addresses.plan,
    );
    let mandate: VelaMandate = harness.fetch_anchor_account(&mandate_address);

    assert_eq!(mandate.next_payment_due, now + trial_period as i64);
    assert_eq!(
        mandate.expiry,
        now + trial_period as i64 + (plan.frequency * plan.max_pulls) as i64
    );
}

#[test]
fn test_subscribe_inactive_plan() {
    let (mut harness, subscriber, mut plan, plan_address, _credential_mint) =
        setup_subscription_fixture();
    plan.status = PlanStatus::Inactive;
    harness.overwrite_anchor_account(&plan_address, &plan);

    let error = harness
        .send_subscribe(&subscriber, plan.plan_id)
        .expect_err("subscribe should reject inactive plans");

    assert!(
        format!("{:?}", error.err).contains("Custom(6007)"),
        "expected PlanNotActive custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_subscribe_cu_budget() {
    let (mut harness, subscriber, plan, _plan_address, _credential_mint) =
        setup_subscription_fixture();

    let meta = harness
        .send_subscribe(&subscriber, plan.plan_id)
        .expect("subscribe should succeed");

    assert!(
        meta.compute_units_consumed < 150_000,
        "subscribe CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}

#[test]
fn test_subscribe_usage_plan_creates_usage_mandate() {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let plan_id = 7u64;
    let settlement_frequency = MIN_FREQUENCY_SECONDS * 2;
    let max_charge_per_period = 75_000_000u64;
    let now = harness.current_timestamp();
    let addresses = harness.derive_usage_plan_addresses(plan_id);
    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");

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
        .send_create_usage_plan(
            plan_id,
            unit_name,
            tiers.clone(),
            max_charge_per_period,
            settlement_frequency,
        )
        .expect("create_usage_plan should succeed");

    let plan: UsagePlan = harness.fetch_anchor_account(&addresses.usage_plan);
    harness
        .send_subscribe_to_plan(
            &subscriber,
            &addresses.usage_plan,
            &merchant_credential_mint,
        )
        .expect("usage plan subscribe should succeed");

    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let mandate_address = harness.derive_mandate_address(&subscriber_pubkey, &addresses.usage_plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&mandate_address);

    assert_eq!(mandate.subscriber, subscriber_pubkey);
    assert_eq!(mandate.plan, addresses.usage_plan);
    assert_eq!(mandate.merchant, harness.merchant_pubkey());
    assert_eq!(mandate.amount, max_charge_per_period);
    assert_eq!(mandate.frequency, settlement_frequency);
    assert_eq!(mandate.start_date, now);
    assert_eq!(mandate.expiry, 0);
    assert_eq!(mandate.max_pulls, u64::MAX);
    assert_eq!(mandate.pulls_executed, 0);
    assert_eq!(mandate.next_payment_due, now + settlement_frequency as i64);
    assert!(matches!(mandate.status, MandateStatus::Active));
    assert!(matches!(mandate.billing_type, BillingType::Usage));

    assert_eq!(plan.max_charge_per_period, max_charge_per_period);
    assert_eq!(plan.settlement_frequency, settlement_frequency);
    assert_eq!(plan.tier_count, tiers.len() as u8);
    assert_eq!(plan.credential_mint, merchant_credential_mint);

    let credential_ata =
        harness.derive_credential_ata(&subscriber_pubkey, &merchant_credential_mint);
    let credential_account =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential account should unpack");
    assert_eq!(
        credential_account.mint.to_string(),
        merchant_credential_mint.to_string()
    );
    assert_eq!(
        credential_account.owner.to_string(),
        subscriber_pubkey.to_string()
    );
    assert_eq!(credential_account.amount, 1);
}

// --- Phase 40-08: Merchant-credential-aware subscribe ---

#[test]
fn test_subscribe_mints_from_merchant_credential() {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());

    // Bootstrap merchant credential first (40-07)
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");

    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();

    // Create plan -- should use merchant credential
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");

    let plan_addresses = harness.derive_plan_addresses(0);
    let plan: VelaPlan = harness.fetch_anchor_account(&plan_addresses.plan);

    // Plan must reference the merchant credential mint
    assert_eq!(
        plan.credential_mint, merchant_credential_mint,
        "plan should reference merchant credential mint"
    );

    // Subscribe -- should mint from merchant credential (D-12)
    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed with merchant credential");

    // Verify credential was minted from the merchant credential mint
    let credential_ata =
        harness.derive_credential_ata(&subscriber_pubkey, &merchant_credential_mint);
    let credential_account =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential account should unpack");
    assert_eq!(
        credential_account.mint.to_string(),
        merchant_credential_mint.to_string(),
        "credential must be from merchant mint (D-12, CRED-02)"
    );
    assert_eq!(credential_account.amount, 1);

    // Verify mandate was created correctly
    let mandate_address = harness.derive_mandate_address(&subscriber_pubkey, &plan_addresses.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&mandate_address);
    assert_eq!(mandate.subscriber, subscriber_pubkey);
    assert!(matches!(mandate.status, MandateStatus::Active));
}

#[test]
fn test_subscribe_legacy_plan_fallback() {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());

    // Seed a pre-bootstrap legacy plan + plan-scoped credential mint (CRED-05 migration path).
    let (_legacy_plan, legacy_credential_mint) = seed_legacy_flat_plan(&mut harness, 0);

    // Subscribe to the legacy plan -- should fall back to plan credential (CRED-05)
    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed with legacy plan credential fallback");

    // Verify credential was minted from the plan-scoped credential mint
    let credential_ata = harness.derive_credential_ata(&subscriber_pubkey, &legacy_credential_mint);
    let credential_account =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential account should unpack");
    assert_eq!(
        credential_account.mint.to_string(),
        legacy_credential_mint.to_string(),
        "credential must be from plan mint for legacy plans (CRED-05)"
    );
    assert_eq!(credential_account.amount, 1);
}

#[test]
fn test_subscribe_credential_persists_across_plan_changes() {
    // CRED-04: credential is merchant-scoped, so plan changes don't require reburn/remint
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());

    // Bootstrap merchant credential
    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");

    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();

    // Create two plans -- both should use the same merchant credential mint
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("first plan should succeed");
    harness
        .send_create_plan(50_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 1)
        .expect("second plan should succeed");

    let plan_a = harness.derive_plan_addresses(0);
    let plan_b = harness.derive_plan_addresses(1);

    let plan_a_record: VelaPlan = harness.fetch_anchor_account(&plan_a.plan);
    let plan_b_record: VelaPlan = harness.fetch_anchor_account(&plan_b.plan);

    // Both plans must reference the same merchant credential mint
    assert_eq!(plan_a_record.credential_mint, merchant_credential_mint);
    assert_eq!(plan_b_record.credential_mint, merchant_credential_mint);

    // Subscribe to both plans
    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe to plan A should succeed");
    harness
        .send_subscribe(&subscriber, 1)
        .expect("subscribe to plan B should succeed");

    // Both subscriptions should create credentials from the same merchant mint
    let credential_ata_a =
        harness.derive_credential_ata(&subscriber_pubkey, &merchant_credential_mint);
    let credential_account =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata_a))
            .expect("credential account should unpack");

    // The credential is merchant-scoped, so it's the same mint for both plans (CRED-04)
    assert_eq!(
        credential_account.mint.to_string(),
        merchant_credential_mint.to_string(),
        "credential mint must be merchant-scoped, not plan-scoped (CRED-04)"
    );
    // Subscriber holds 2 tokens from the same merchant credential mint
    assert_eq!(
        credential_account.amount, 2,
        "subscriber should have 2 credential tokens from the same merchant mint"
    );
}

#[test]
fn test_subscribe_uses_merchant_mandate_counter_for_reseeded_pda() {
    let mut harness = TestHarness::new();
    let subscriber_a = harness.create_wallet();
    let subscriber_b = harness.create_wallet();
    let subscriber_a_pubkey = Pubkey::new_from_array(subscriber_a.pubkey().to_bytes());
    let subscriber_b_pubkey = Pubkey::new_from_array(subscriber_b.pubkey().to_bytes());

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("create_plan should succeed");

    let merchant = harness.merchant_pubkey();
    let (merchant_state_key, _) = Pubkey::find_program_address(
        &[MerchantState::SEED_PREFIX, merchant.as_ref()],
        &vela_protocol::ID,
    );
    let merchant_state_before: MerchantState = harness.fetch_anchor_account(&merchant_state_key);
    assert_eq!(merchant_state_before.mandate_counter, 0);

    harness
        .send_subscribe(&subscriber_a, 0)
        .expect("subscribe A should succeed");
    let merchant_state_after_a: MerchantState = harness.fetch_anchor_account(&merchant_state_key);
    assert_eq!(merchant_state_after_a.mandate_counter, 1);

    harness
        .send_subscribe(&subscriber_b, 0)
        .expect("subscribe B should succeed");
    let merchant_state_after_b: MerchantState = harness.fetch_anchor_account(&merchant_state_key);
    assert_eq!(merchant_state_after_b.mandate_counter, 2);

    let first_mandate = harness.derive_mandate_address_by_index(&subscriber_a_pubkey, &merchant, 0);
    let second_mandate =
        harness.derive_mandate_address_by_index(&subscriber_b_pubkey, &merchant, 1);
    assert_ne!(first_mandate, second_mandate);

    let first_record: VelaMandate = harness.fetch_anchor_account(&first_mandate);
    let second_record: VelaMandate = harness.fetch_anchor_account(&second_mandate);
    assert_eq!(first_record.subscriber, subscriber_a_pubkey);
    assert_eq!(second_record.subscriber, subscriber_b_pubkey);
}

#[test]
fn test_subscribe_uses_merchant_index_mandate_seed() {
    let (mut harness, subscriber, plan, plan_address, _credential_mint) =
        setup_subscription_fixture();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let addresses = harness.derive_plan_addresses(plan.plan_id);

    let merchant_state_before: MerchantState =
        harness.fetch_anchor_account(&addresses.merchant_state);
    let expected_v2_mandate = harness.derive_mandate_address_v2(
        &subscriber_pubkey,
        &harness.merchant_pubkey(),
        merchant_state_before.mandate_counter,
    );
    let legacy_plan_seeded_mandate =
        harness.derive_mandate_address(&subscriber_pubkey, &plan_address);

    harness
        .send_subscribe(&subscriber, plan.plan_id)
        .expect("subscribe should succeed");

    let merchant_state_after: MerchantState =
        harness.fetch_anchor_account(&addresses.merchant_state);
    assert_eq!(
        merchant_state_after.mandate_counter,
        merchant_state_before.mandate_counter + 1,
        "merchant mandate_counter must increment monotonically"
    );

    assert!(
        harness
            .svm
            .get_account(&solana_address::Address::from(
                expected_v2_mandate.to_bytes()
            ))
            .is_some(),
        "subscribe must create a mandate at the merchant/index-seeded address",
    );
    assert!(
        harness
            .svm
            .get_account(&solana_address::Address::from(
                legacy_plan_seeded_mandate.to_bytes()
            ))
            .is_none(),
        "new mandates must not use plan-seeded mandate PDA",
    );
}
