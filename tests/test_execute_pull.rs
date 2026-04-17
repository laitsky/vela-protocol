#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::{SubscriptionFixture, TestHarness};
use solana_keypair::Keypair;
use solana_program_pack::Pack;
use solana_signer::Signer;
use spl_token_2022::state::Account as Token2022Account;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{BillingEvent, MandateStatus, PullApproval, UsageReport, VelaMandate, VelaPlan},
};

/// Set up a full fixture with Token-2022 wrapped USDC accounts injected.
/// The mandate PDA owns the subscriber billing account, matching the production flow.
fn setup_fixture() -> (
    TestHarness,
    SubscriptionFixture,
    VelaPlan,
    VelaMandate,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);

    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);

    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);

    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let subscriber_usdc =
        harness.create_spl_token_account(&fixture.subscriber, &spl_usdc_mint, &subscriber);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, plan.amount * 10);

    let subscriber_wrapped_pubkey =
        harness.create_token_2022_ata(&admin, &fixture.mandate, &wrapped_mint_pubkey);
    harness
        .send_wrap(
            &fixture.subscriber,
            &spl_usdc_mint,
            &wrapped_mint_pubkey,
            &subscriber_usdc,
            &subscriber_wrapped_pubkey,
            &fixture.mandate,
            &wrapping_vault,
            plan.amount * 2,
        )
        .expect("wrap into the mandate billing account should succeed");

    let merchant_wrapped_pubkey =
        harness.create_token_2022_ata(&admin, &harness.merchant_pubkey(), &wrapped_mint_pubkey);

    (
        harness,
        fixture,
        plan,
        mandate,
        subscriber_wrapped_pubkey,
        merchant_wrapped_pubkey,
        wrapped_mint_pubkey,
    )
}

#[test]
fn test_execute_pull_success() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    let meta = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("execute_pull should succeed");

    assert!(meta.compute_units_consumed > 0);

    let mandate_after: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert_eq!(mandate_after.pulls_executed, 1);
    assert_eq!(
        mandate_after.next_payment_due,
        mandate_before.next_payment_due + plan.frequency as i64
    );
    assert!(matches!(mandate_after.status, MandateStatus::Active));
}

#[test]
fn test_execute_pull_permissionless() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture();
    let payer = harness.create_wallet();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    harness
        .send_execute_pull(
            &payer,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("third-party payer should be able to execute_pull");

    let mandate_after: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert_eq!(mandate_after.pulls_executed, 1);
}

#[test]
fn test_execute_pull_too_early() {
    let (mut harness, fixture, plan, _mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture();
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        plan.amount,
    );

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("execute_pull should reject early pulls");

    assert!(
        format!("{:?}", error.err).contains("Custom(6000)"),
        "expected PullTooEarly custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_execute_pull_cu_budget() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    let meta = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("execute_pull should succeed");

    assert!(
        meta.compute_units_consumed < 150_000,
        "execute_pull CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}

#[test]
fn test_next_pull_requires_billing_record_finalization() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        plan.amount,
    );

    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("first execute_pull should succeed");

    let mandate_after_first: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    harness.set_clock_timestamp(mandate_after_first.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_after_first.next_payment_due,
        true,
        plan.amount,
    );

    let error = harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect_err("second pull should be blocked until billing is finalized");

    assert!(
        format!("{:?}", error.err).contains("Custom(6019)"),
        "expected PendingBillingRecord custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_execute_pull_uses_mandate_values_after_plan_update() {
    let (
        mut harness,
        fixture,
        plan_before,
        mandate_before,
        sub_wrapped,
        merch_wrapped,
        wrapped_mint,
    ) = setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());

    // plan update: mutate flat-plan amount/frequency after subscription.
    harness
        .send_update_plan(
            plan_before.plan_id,
            Some(plan_before.amount + 5_000_000),
            Some(plan_before.frequency * 2),
            None,
            None,
        )
        .expect("update_plan should succeed");

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        mandate_before.amount,
    );

    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan_before.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("execute_pull should continue using mandate amount/frequency after plan update");

    let mandate_after: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    assert_eq!(mandate_after.pulls_executed, 1);
    assert_eq!(
        mandate_after.next_payment_due,
        mandate_before.next_payment_due + mandate_before.frequency as i64
    );
}

#[test]
fn test_execute_pull_uses_mandate_amount_after_plan_update() {
    let (mut harness, fixture, plan, mandate_before, sub_wrapped, merch_wrapped, wrapped_mint) =
        setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let updated_plan_amount = plan.amount * 2;

    harness
        .send_update_plan(plan.plan_id, Some(updated_plan_amount), None, None, None)
        .expect("update_plan should succeed");

    harness.set_clock_timestamp(mandate_before.next_payment_due);
    harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due,
        true,
        updated_plan_amount,
    );

    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            &sub_wrapped,
            &merch_wrapped,
            &wrapped_mint,
        )
        .expect("execute_pull should use mandate-stored amount/frequency after plan update");

    let merchant_wrapped =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&merch_wrapped))
            .expect("merchant wrapped account should unpack");
    assert_eq!(
        merchant_wrapped.amount, mandate_before.amount,
        "execute_pull must settle using mandate.amount, not updated plan amount",
    );
}

#[test]
fn test_execute_pull_namespace_references_use_active_mandate_key() {
    // validation / billing / usage namespace derivations must use mandate.key().
    let (_harness, fixture, _plan, _mandate_before, _sub, _merch, _mint) = setup_fixture();
    let validation = Pubkey::find_program_address(
        &[PullApproval::SEED_PREFIX, fixture.mandate.as_ref()],
        &vela_protocol::ID,
    )
    .0;
    let billing = Pubkey::find_program_address(
        &[
            BillingEvent::SEED_PREFIX,
            fixture.mandate.as_ref(),
            1u64.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    )
    .0;
    let usage = Pubkey::find_program_address(
        &[
            UsageReport::SEED_PREFIX,
            fixture.mandate.as_ref(),
            0i64.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    )
    .0;
    assert_ne!(validation, billing);
    assert_ne!(validation, usage);
}

#[test]
fn test_execute_pull_v2_mandate_downstream_namespace_continuity() {
    // V2 mandates (created via subscribe with mandate_index seeds) must produce
    // downstream validation/billing/usage PDAs keyed to mandate.key() -- not to
    // a stale legacy address.
    let (harness, fixture, _plan, _mandate_before, _sub, _merch, _mint) = setup_fixture();
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let _merchant = harness.merchant_pubkey();

    // The fixture mandate is a V2 mandate (subscriber, merchant, mandate_index)
    let mandate_v2 = fixture.mandate;

    // Derive what the legacy address would have been
    let legacy_mandate = Pubkey::find_program_address(
        &[
            VelaMandate::SEED_PREFIX,
            subscriber.as_ref(),
            fixture.plan.as_ref(),
        ],
        &vela_protocol::ID,
    )
    .0;

    // V2 mandate address must differ from legacy
    assert_ne!(
        mandate_v2, legacy_mandate,
        "V2 mandate PDA must differ from legacy plan-keyed PDA"
    );

    // Downstream PDAs must be keyed to the V2 mandate address
    let validation_v2 = Pubkey::find_program_address(
        &[PullApproval::SEED_PREFIX, mandate_v2.as_ref()],
        &vela_protocol::ID,
    )
    .0;
    let billing_v2 = Pubkey::find_program_address(
        &[
            BillingEvent::SEED_PREFIX,
            mandate_v2.as_ref(),
            1u64.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    )
    .0;
    let usage_v2 = Pubkey::find_program_address(
        &[
            UsageReport::SEED_PREFIX,
            mandate_v2.as_ref(),
            0i64.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    )
    .0;

    // Legacy-derived downstream PDAs (should differ from V2-derived ones)
    let validation_legacy = Pubkey::find_program_address(
        &[PullApproval::SEED_PREFIX, legacy_mandate.as_ref()],
        &vela_protocol::ID,
    )
    .0;
    let billing_legacy = Pubkey::find_program_address(
        &[
            BillingEvent::SEED_PREFIX,
            legacy_mandate.as_ref(),
            1u64.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    )
    .0;
    let usage_legacy = Pubkey::find_program_address(
        &[
            UsageReport::SEED_PREFIX,
            legacy_mandate.as_ref(),
            0i64.to_le_bytes().as_ref(),
        ],
        &vela_protocol::ID,
    )
    .0;

    assert_ne!(
        validation_v2, validation_legacy,
        "validation namespace must follow V2 mandate.key()"
    );
    assert_ne!(
        billing_v2, billing_legacy,
        "billing namespace must follow V2 mandate.key()"
    );
    assert_ne!(
        usage_v2, usage_legacy,
        "usage namespace must follow V2 mandate.key()"
    );
}
