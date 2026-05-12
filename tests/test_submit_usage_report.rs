#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use litesvm::types::FailedTransactionMetadata;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    errors::VelaError,
    instructions::plan_account::usage_plan_terms_hash_from_parts,
    state::{PricingTier, UsagePlan, UsageReport, VelaMandate},
};

fn make_ciphertexts(count: usize, seed: u8) -> Vec<[u8; 32]> {
    (0..count)
        .map(|index| {
            let mut ciphertext = [seed; 32];
            ciphertext[0] = index as u8;
            ciphertext
        })
        .collect()
}

fn setup_usage_mandate(
    tiers: Vec<PricingTier>,
) -> (
    TestHarness,
    solana_keypair::Keypair,
    Pubkey,
    Pubkey,
    VelaMandate,
) {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let plan_id = 71u64;
    let addresses = harness.derive_usage_plan_addresses(plan_id);
    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_usage_plan(plan_id, [0u8; 32], tiers, 75_000_000, MIN_FREQUENCY_SECONDS)
        .expect("create_usage_plan should succeed");

    let merchant_state: vela_protocol::state::MerchantState =
        harness.fetch_anchor_account(&addresses.merchant_state);
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let mandate_address = harness.derive_mandate_address_by_index(
        &subscriber_pubkey,
        &harness.merchant_pubkey(),
        merchant_state.mandate_counter,
    );
    harness
        .send_subscribe_to_plan(
            &subscriber,
            &addresses.usage_plan,
            &merchant_credential_mint,
        )
        .expect("usage subscribe should succeed");

    let mandate = harness.fetch_anchor_account(&mandate_address);
    (
        harness,
        subscriber,
        addresses.usage_plan,
        mandate_address,
        mandate,
    )
}

fn error_has(failure: &FailedTransactionMetadata, needle: &str) -> bool {
    format!("{:?}", failure.err).contains(needle)
        || failure.meta.logs.iter().any(|log| log.contains(needle))
}

fn current_period(mandate: &VelaMandate) -> (i64, i64) {
    (
        mandate.next_payment_due - mandate.frequency as i64,
        mandate.next_payment_due,
    )
}

#[test]
fn usage_report_accepts_only_usage_units_ciphertext() {
    let tiers = vec![
        PricingTier {
            up_to: 1_000,
            rate_per_unit: 100,
            _padding: 0,
        },
        PricingTier {
            up_to: 0,
            rate_per_unit: 50,
            _padding: 0,
        },
    ];
    let (mut harness, _subscriber, usage_plan, mandate_address, mandate) =
        setup_usage_mandate(tiers);
    let (period_start, period_end) = current_period(&mandate);
    harness.set_clock_timestamp(period_end);
    let ciphertext = make_ciphertexts(1, 9);
    let nonce = 42u128;
    let pub_key = [7u8; 32];

    harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start,
            period_end,
            ciphertext.clone(),
            nonce,
            pub_key,
        )
        .expect("tiered usage report should submit");

    let usage_report_address = harness.derive_usage_report_address(&mandate_address, period_start);
    let report: UsageReport = harness.fetch_anchor_account(&usage_report_address);
    let plan: UsagePlan = harness.fetch_anchor_account(&usage_plan);
    let expected_terms_hash = usage_plan_terms_hash_from_parts(
        &usage_plan,
        &plan.merchant,
        plan.plan_id,
        &plan.tiers,
        plan.tier_count,
        plan.max_charge_per_period,
        plan.settlement_frequency,
    );
    assert_eq!(report.ciphertext_count, 1);
    assert_eq!(report.computation_ciphertext[0], ciphertext[0]);
    assert_eq!(report.computation_ciphertext[1], expected_terms_hash);
    assert_eq!(report.nonce, nonce);
    assert_eq!(report.pub_key, pub_key);
}

#[test]
fn submit_usage_report_rejects_wrong_ciphertext_shape_for_plan() {
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let (mut harness, _subscriber, usage_plan, mandate_address, mandate) =
        setup_usage_mandate(tiers);
    let (period_start, period_end) = current_period(&mandate);
    harness.set_clock_timestamp(period_end);

    let error = harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start,
            period_end,
            make_ciphertexts(13, 3),
            1,
            [1u8; 32],
        )
        .expect_err("usage report must reject pricing ciphertexts");

    assert!(
        format!("{:?}", error.err).contains("Custom(6018)"),
        "expected InvalidCiphertextInput, got {:?}",
        error.err,
    );
}

#[test]
fn subscriber_cannot_preempt_usage_report_period() {
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let (mut harness, subscriber, usage_plan, mandate_address, mandate) =
        setup_usage_mandate(tiers);
    let (period_start, period_end) = current_period(&mandate);
    harness.set_clock_timestamp(period_end);
    let subscriber_ciphertext = make_ciphertexts(1, 3);

    let err = harness
        .send_submit_usage_report_as(
            &subscriber,
            &mandate_address,
            &usage_plan,
            period_start,
            period_end,
            subscriber_ciphertext,
            1,
            [1u8; 32],
        )
        .expect_err("subscriber must not be able to initialize the usage report PDA");

    let unauthorized_keeper = format!(
        "Custom({})",
        anchor_lang::error::ERROR_CODE_OFFSET + VelaError::UnauthorizedKeeper as u32
    );
    assert!(
        error_has(&err, &unauthorized_keeper) || error_has(&err, "UnauthorizedKeeper"),
        "expected UnauthorizedKeeper, got err={:?}, logs={:?}",
        err.err,
        err.meta.logs
    );

    let merchant_ciphertext = make_ciphertexts(1, 9);
    let merchant_nonce = 42u128;
    let merchant_pub_key = [7u8; 32];
    harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start,
            period_end,
            merchant_ciphertext.clone(),
            merchant_nonce,
            merchant_pub_key,
        )
        .expect("merchant should still be able to submit the canonical usage report");

    let usage_report_address = harness.derive_usage_report_address(&mandate_address, period_start);
    let report: UsageReport = harness.fetch_anchor_account(&usage_report_address);
    assert_eq!(report.ciphertext_count, 1);
    assert_eq!(report.computation_ciphertext[0], merchant_ciphertext[0]);
    assert_eq!(report.nonce, merchant_nonce);
    assert_eq!(report.pub_key, merchant_pub_key);
}

#[test]
fn submit_usage_report_rejects_open_current_period() {
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let (mut harness, _subscriber, usage_plan, mandate_address, mandate) =
        setup_usage_mandate(tiers);
    let (period_start, period_end) = current_period(&mandate);
    harness.set_clock_timestamp(period_end - 1);

    let err = harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start,
            period_end,
            make_ciphertexts(1, 4),
            1,
            [1u8; 32],
        )
        .expect_err("usage report should not submit before the period closes");

    assert!(
        error_has(&err, "PullTooEarly") || format!("{:?}", err.err).contains("Custom(6000)"),
        "expected PullTooEarly, got err={:?}, logs={:?}",
        err.err,
        err.meta.logs
    );
}

#[test]
fn submit_usage_report_rejects_wrong_period_bounds() {
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let (mut harness, _subscriber, usage_plan, mandate_address, mandate) =
        setup_usage_mandate(tiers);
    let (period_start, period_end) = current_period(&mandate);
    harness.set_clock_timestamp(period_end);

    let err = harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start + 1,
            period_end,
            make_ciphertexts(1, 5),
            1,
            [1u8; 32],
        )
        .expect_err("usage report should reject non-current period bounds");

    assert!(
        error_has(&err, "PeriodMismatch") || format!("{:?}", err.err).contains("Custom(6056)"),
        "expected PeriodMismatch, got err={:?}, logs={:?}",
        err.err,
        err.meta.logs
    );
}

#[test]
fn usage_execute_pull_requires_usage_report_remaining_account() {
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let (mut harness, subscriber, usage_plan, mandate_address, mandate) =
        setup_usage_mandate(tiers);
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let (period_start, period_end) = current_period(&mandate);
    harness.set_clock_timestamp(period_end);
    harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start,
            period_end,
            make_ciphertexts(1, 6),
            1,
            [1u8; 32],
        )
        .expect("usage report should submit");

    let mut funded_mandate: VelaMandate = harness.fetch_anchor_account(&mandate_address);
    funded_mandate.credit_balance = 5_000;
    harness.overwrite_anchor_account(&mandate_address, &funded_mandate);
    harness.create_pull_approval_with_amount(&mandate_address, period_end + 600, true, 5_000);

    let err = harness
        .send_execute_usage_pull(
            &subscriber,
            &subscriber_pubkey,
            &usage_plan,
            &mandate_address,
            None,
        )
        .expect_err("usage execute_pull must require the current usage report account");

    assert!(
        error_has(&err, "PeriodMismatch") || format!("{:?}", err.err).contains("Custom(6056)"),
        "expected PeriodMismatch, got err={:?}, logs={:?}",
        err.err,
        err.meta.logs
    );

    let usage_report_address = harness.derive_usage_report_address(&mandate_address, period_start);
    let report: UsageReport = harness.fetch_anchor_account(&usage_report_address);
    assert!(
        !report.settled,
        "missing-report failure must leave report unpaid"
    );
}

#[test]
fn usage_execute_pull_marks_report_settled_after_success() {
    let tiers = vec![PricingTier {
        up_to: 0,
        rate_per_unit: 100,
        _padding: 0,
    }];
    let (mut harness, subscriber, usage_plan, mandate_address, mandate_before) =
        setup_usage_mandate(tiers);
    let subscriber_pubkey = Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let (period_start, period_end) = current_period(&mandate_before);
    harness.set_clock_timestamp(period_end);
    harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start,
            period_end,
            make_ciphertexts(1, 7),
            2,
            [2u8; 32],
        )
        .expect("usage report should submit");
    let usage_report_address = harness.derive_usage_report_address(&mandate_address, period_start);
    let report_before: UsageReport = harness.fetch_anchor_account(&usage_report_address);
    assert!(
        !report_before.settled,
        "submitted/computed usage report must remain unpaid before execute_pull"
    );

    let approved_amount = 5_000;
    let mut funded_mandate: VelaMandate = harness.fetch_anchor_account(&mandate_address);
    funded_mandate.credit_balance = approved_amount;
    harness.overwrite_anchor_account(&mandate_address, &funded_mandate);
    harness.create_pull_approval_with_amount(
        &mandate_address,
        period_end + 600,
        true,
        approved_amount,
    );

    harness
        .send_execute_usage_pull(
            &subscriber,
            &subscriber_pubkey,
            &usage_plan,
            &mandate_address,
            Some(&usage_report_address),
        )
        .expect("usage execute_pull should settle an unpaid report");

    let report_after: UsageReport = harness.fetch_anchor_account(&usage_report_address);
    assert!(
        report_after.settled,
        "execute_pull must mark the report paid"
    );

    let mandate_after: VelaMandate = harness.fetch_anchor_account(&mandate_address);
    assert_eq!(
        mandate_after.next_payment_due,
        mandate_before.next_payment_due + mandate_before.frequency as i64
    );
    assert_eq!(
        mandate_after.pulls_executed,
        mandate_before.pulls_executed + 1
    );
    assert_eq!(
        mandate_after.last_billing_recorded_pull,
        mandate_after.pulls_executed
    );

    harness.set_clock_timestamp(mandate_after.next_payment_due);
    harness.create_pull_approval_with_period_and_amount(
        &mandate_address,
        period_start,
        period_end,
        mandate_after.next_payment_due + 600,
        true,
        approved_amount,
    );
    let err = harness
        .send_execute_usage_pull(
            &subscriber,
            &subscriber_pubkey,
            &usage_plan,
            &mandate_address,
            Some(&usage_report_address),
        )
        .expect_err("same paid report must not be payable twice");
    assert!(
        error_has(&err, "PeriodMismatch")
            || error_has(&err, "UsageReportAlreadySettled")
            || format!("{:?}", err.err).contains("Custom(6056)")
            || format!("{:?}", err.err).contains("Custom(6061)"),
        "expected stale/settled report rejection, got err={:?}, logs={:?}",
        err.err,
        err.meta.logs
    );
}
