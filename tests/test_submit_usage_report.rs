#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_signer::Signer;
use vela_protocol::{
    constants::{MAX_USAGE_COMPUTATION_CIPHERTEXTS, MIN_FREQUENCY_SECONDS},
    state::{PricingTier, UsageReport, VelaMandate},
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

fn setup_usage_mandate(tiers: Vec<PricingTier>) -> (TestHarness, Pubkey, Pubkey, VelaMandate) {
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
    (harness, addresses.usage_plan, mandate_address, mandate)
}

#[test]
fn submit_usage_report_stores_committed_tiered_ciphertext() {
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
    let (mut harness, usage_plan, mandate_address, mandate) = setup_usage_mandate(tiers);
    let period_start = mandate.next_payment_due;
    let ciphertext = make_ciphertexts(MAX_USAGE_COMPUTATION_CIPHERTEXTS, 9);
    let nonce = 42u128;
    let pub_key = [7u8; 32];

    harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start,
            period_start + MIN_FREQUENCY_SECONDS as i64,
            ciphertext.clone(),
            nonce,
            pub_key,
        )
        .expect("tiered usage report should submit");

    let usage_report_address = harness.derive_usage_report_address(&mandate_address, period_start);
    let report: UsageReport = harness.fetch_anchor_account(&usage_report_address);
    assert_eq!(usize::from(report.ciphertext_count), ciphertext.len());
    assert_eq!(
        &report.computation_ciphertext[..ciphertext.len()],
        &ciphertext[..]
    );
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
    let (mut harness, usage_plan, mandate_address, mandate) = setup_usage_mandate(tiers);
    let period_start = mandate.next_payment_due;

    let error = harness
        .send_submit_usage_report(
            &mandate_address,
            &usage_plan,
            period_start,
            period_start + MIN_FREQUENCY_SECONDS as i64,
            make_ciphertexts(MAX_USAGE_COMPUTATION_CIPHERTEXTS, 3),
            1,
            [1u8; 32],
        )
        .expect_err("single-tier usage report must reject tiered ciphertext shape");

    assert!(
        format!("{:?}", error.err).contains("Custom(6018)"),
        "expected InvalidCiphertextInput, got {:?}",
        error.err,
    );
}
