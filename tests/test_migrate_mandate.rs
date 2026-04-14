#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::{to_address, TestHarness};
use solana_account::Account;
use solana_address::Address;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{
        BillingEvent, BillingType, CURRENT_ACCOUNT_VERSION, MandateStatus, MerchantState,
        PullApproval, UsageReport, VelaMandate,
    },
};

fn build_v1_mandate_bytes(
    subscriber: &Pubkey,
    plan: &Pubkey,
    merchant: &Pubkey,
    amount: u64,
    frequency: u64,
    start_date: i64,
    expiry: i64,
    max_pulls: u64,
    pulls_executed: u64,
    next_payment_due: i64,
    last_pull_at: i64,
    last_billing_recorded_pull: u64,
    validation_request_nonce: u64,
    billing_request_nonce: u64,
    status: MandateStatus,
    bump: u8,
    billing_type: BillingType,
) -> Vec<u8> {
    let mut data = Vec::new();
    anchor_lang::AnchorSerialize::serialize(subscriber, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(plan, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(merchant, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&amount, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&frequency, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&start_date, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&expiry, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&max_pulls, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&pulls_executed, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&next_payment_due, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&last_pull_at, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&last_billing_recorded_pull, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&validation_request_nonce, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&billing_request_nonce, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&status, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&bump, &mut data).unwrap();
    anchor_lang::AnchorSerialize::serialize(&billing_type, &mut data).unwrap();
    data
}

fn setup_fixture(
    validation_request_nonce: u64,
) -> (TestHarness, Pubkey, Pubkey, Pubkey, Pubkey, u64) {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let merchant = harness.merchant_pubkey();
    let plan_id = 0u64;
    let amount = 25_000_000u64;
    let frequency = MIN_FREQUENCY_SECONDS;
    let max_pulls = 4u64;

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(amount, frequency, 0, max_pulls, plan_id)
        .expect("create_plan should succeed");
    let admin = harness.merchant.insecure_clone();
    harness.init_protocol_config(&admin);

    let plan = harness.derive_plan_addresses(plan_id).plan;
    let (legacy_mandate, legacy_bump) = Pubkey::find_program_address(
        &[
            vela_protocol::state::VelaMandate::SEED_PREFIX,
            subscriber_pubkey.as_ref(),
            plan.as_ref(),
        ],
        &vela_protocol::ID,
    );

    let now = harness.current_timestamp();
    let body = build_v1_mandate_bytes(
        &subscriber_pubkey,
        &plan,
        &merchant,
        amount,
        frequency,
        now,
        now + (frequency * max_pulls) as i64,
        max_pulls,
        0,
        now + frequency as i64,
        0,
        0,
        validation_request_nonce,
        0,
        MandateStatus::Active,
        legacy_bump,
        BillingType::Flat,
    );

    use anchor_lang::Discriminator;
    let mut full_data = Vec::new();
    full_data.extend_from_slice(&VelaMandate::DISCRIMINATOR);
    full_data.extend_from_slice(&body);
    harness
        .svm
        .set_account(
            to_address(legacy_mandate),
            Account {
                lamports: 2_000_000,
                data: full_data,
                owner: harness.program_id,
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("legacy mandate should be created");

    let (merchant_state, _) = Pubkey::find_program_address(
        &[vela_protocol::state::MerchantState::SEED_PREFIX, merchant.as_ref()],
        &vela_protocol::ID,
    );
    let merchant_state_data: MerchantState = harness.fetch_anchor_account(&merchant_state);
    let mandate_index = merchant_state_data.mandate_counter;
    let migrated_mandate =
        harness.derive_mandate_address_by_index(&subscriber_pubkey, &merchant, mandate_index);

    (
        harness,
        subscriber_pubkey,
        plan,
        legacy_mandate,
        migrated_mandate,
        mandate_index,
    )
}

#[test]
fn test_migrate_mandate_is_admin_only() {
    // admin-only: only protocol admin may run migrate_mandate.
    let (mut harness, subscriber, plan, legacy_mandate, migrated_mandate, _index) =
        setup_fixture(0);
    let attacker = harness.create_wallet();

    let error = harness
        .send_migrate_mandate_as(
            &attacker,
            &subscriber,
            &plan,
            &legacy_mandate,
            &migrated_mandate,
        )
        .expect_err("non-admin migration must fail");

    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom unauthorized error, got {:?}",
        error.err
    );
}

#[test]
fn test_migrate_mandate_close_v1_open_v2_preserves_fields() {
    // close-V1 and open-V2 migration path (per-account compatibility path; not bulk backfill).
    let (mut harness, subscriber, plan, legacy_mandate, migrated_mandate, mandate_index) =
        setup_fixture(0);

    harness
        .send_migrate_mandate(&subscriber, &plan, &legacy_mandate, &migrated_mandate)
        .expect("admin should migrate mandate");

    assert!(
        harness
            .svm
            .get_account(&Address::from(legacy_mandate.to_bytes()))
            .is_none(),
        "legacy mandate must be closed after migration"
    );
    assert!(
        harness
            .svm
            .get_account(&Address::from(migrated_mandate.to_bytes()))
            .is_some(),
        "migrated mandate must be created at V2 address"
    );

    let migrated: VelaMandate = harness.fetch_anchor_account(&migrated_mandate);
    assert_eq!(migrated.subscriber, subscriber);
    assert_eq!(migrated.plan, plan);
    assert_eq!(migrated.merchant, harness.merchant_pubkey());
    assert_eq!(migrated.amount, 25_000_000);
    assert_eq!(migrated.frequency, MIN_FREQUENCY_SECONDS);
    assert_eq!(migrated.max_pulls, 4);
    assert_eq!(migrated.mandate_index, mandate_index);
    assert_eq!(migrated.version, CURRENT_ACCOUNT_VERSION);
}

#[test]
fn test_migrate_mandate_rejects_stranded_state() {
    // stranded-state guard: reject if PullApproval / UsageReport / BillingEvent namespaces
    // could be stranded under the old mandate address.
    let (mut harness, subscriber, plan, legacy_mandate, migrated_mandate, _index) =
        setup_fixture(1);

    let error = harness
        .send_migrate_mandate(&subscriber, &plan, &legacy_mandate, &migrated_mandate)
        .expect_err("migration with stranded dependent state must fail");
    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom stranded-state rejection, got {:?}",
        error.err
    );
    assert!(
        harness
            .svm
            .get_account(&Address::from(legacy_mandate.to_bytes()))
            .is_some(),
        "legacy mandate must remain when migration is rejected"
    );
}

#[test]
fn test_migrated_mandate_keys_validation_billing_usage_namespaces() {
    // downstream namespace continuity: validation, billing, and usage should key off mandate.key().
    let (mut harness, subscriber, plan, legacy_mandate, migrated_mandate, _index) =
        setup_fixture(0);

    harness
        .send_migrate_mandate(&subscriber, &plan, &legacy_mandate, &migrated_mandate)
        .expect("migration should succeed");

    let validation_approval = Pubkey::find_program_address(
        &[PullApproval::SEED_PREFIX, migrated_mandate.as_ref()],
        &vela_protocol::ID,
    )
    .0;
    let billing_event = Pubkey::find_program_address(
        &[BillingEvent::SEED_PREFIX, migrated_mandate.as_ref(), 1u64.to_le_bytes().as_ref()],
        &vela_protocol::ID,
    )
    .0;
    let usage_report = Pubkey::find_program_address(
        &[
            UsageReport::SEED_PREFIX,
            migrated_mandate.as_ref(),
            0i64.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    )
    .0;

    assert_ne!(
        validation_approval,
        Pubkey::find_program_address(
            &[PullApproval::SEED_PREFIX, legacy_mandate.as_ref()],
            &vela_protocol::ID
        )
        .0,
        "validation namespace must follow the active mandate.key()",
    );
    assert_ne!(
        billing_event,
        Pubkey::find_program_address(
            &[BillingEvent::SEED_PREFIX, legacy_mandate.as_ref(), 1u64.to_le_bytes().as_ref()],
            &vela_protocol::ID
        )
        .0,
        "billing namespace must follow the active mandate.key()",
    );
    assert_ne!(
        usage_report,
        Pubkey::find_program_address(
            &[
                UsageReport::SEED_PREFIX,
                legacy_mandate.as_ref(),
                0i64.to_le_bytes().as_ref(),
            ],
            &vela_protocol::ID
        )
        .0,
        "usage namespace must follow the active mandate.key()",
    );
}
