#![allow(dead_code)]

#[path = "helpers/mod.rs"]
pub mod helpers;

use std::any::type_name;

use anchor_lang::{AnchorDeserialize, Discriminator};
use anchor_lang::__private::base64::{engine::general_purpose::STANDARD, Engine as _};
use helpers::TestHarness;
use litesvm::types::{FailedTransactionMetadata, TransactionMetadata};
use solana_keypair::Keypair;
use solana_program_pack::Pack;
use solana_signer::Signer;
use spl_token_2022::state::Account as Token2022Account;
use vela_protocol::state::{MandateUpgradeCancelled, MandateUpgradeFinalized, MandateUpgradeInitiated, VelaMandate, VelaPlan};

pub const PERIOD_SECONDS: u64 = 30 * 24 * 60 * 60;

pub struct PeriodicUpgradeFixture {
    pub harness: TestHarness,
    pub subscriber: Keypair,
    pub mandate: anchor_lang::prelude::Pubkey,
    pub plan_a: anchor_lang::prelude::Pubkey,
    pub plan_b: anchor_lang::prelude::Pubkey,
    pub plan_a_state: VelaPlan,
    pub plan_b_state: VelaPlan,
    pub subscriber_wrapped: anchor_lang::prelude::Pubkey,
    pub merchant_wrapped: anchor_lang::prelude::Pubkey,
    pub wrapped_mint: anchor_lang::prelude::Pubkey,
    pub spl_usdc_mint: anchor_lang::prelude::Pubkey,
}

pub fn setup_periodic_upgrade_fixture(
    amount_a: u64,
    amount_b: u64,
) -> PeriodicUpgradeFixture {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey = anchor_lang::prelude::Pubkey::new_from_array(subscriber.pubkey().to_bytes());

    harness
        .send_init_merchant_credential()
        .expect("init_merchant_credential should succeed");
    harness
        .send_create_plan(amount_a, PERIOD_SECONDS, 0, 6, 0)
        .expect("create plan A should succeed");
    harness
        .send_create_plan(amount_b, PERIOD_SECONDS, 0, 6, 1)
        .expect("create plan B should succeed");
    harness
        .send_subscribe(&subscriber, 0)
        .expect("subscribe should succeed");

    let plan_a = harness.derive_plan_addresses(0).plan;
    let plan_b = harness.derive_plan_addresses(1).plan;
    let mandate = harness.derive_mandate_address(&subscriber_pubkey, &plan_a);
    let plan_a_state: VelaPlan = harness.fetch_anchor_account(&plan_a);
    let plan_b_state: VelaPlan = harness.fetch_anchor_account(&plan_b);

    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);

    let wrapped_mint = Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);

    let subscriber_usdc =
        harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, amount_a * 10);

    let subscriber_wrapped =
        harness.create_token_2022_ata(&admin, &mandate, &wrapped_mint_pubkey);
    harness
        .send_wrap(
            &subscriber,
            &spl_usdc_mint,
            &wrapped_mint_pubkey,
            &subscriber_usdc,
            &subscriber_wrapped,
            &mandate,
            &wrapping_vault,
            amount_a * 4,
        )
        .expect("wrap should succeed");
    let merchant_wrapped =
        harness.create_token_2022_ata(&admin, &harness.merchant_pubkey(), &wrapped_mint_pubkey);

    PeriodicUpgradeFixture {
        harness,
        subscriber,
        mandate,
        plan_a,
        plan_b,
        plan_a_state,
        plan_b_state,
        subscriber_wrapped,
        merchant_wrapped,
        wrapped_mint: wrapped_mint_pubkey,
        spl_usdc_mint,
    }
}

pub fn fetch_mandate(harness: &TestHarness, mandate: &anchor_lang::prelude::Pubkey) -> VelaMandate {
    harness.fetch_anchor_account(mandate)
}

pub fn wrapped_balance(
    harness: &TestHarness,
    account: &anchor_lang::prelude::Pubkey,
) -> u64 {
    Token2022Account::unpack_from_slice(&harness.fetch_account_data(account))
        .expect("token-2022 account should unpack")
        .amount
}

pub fn decode_event<T>(metadata: &TransactionMetadata) -> Vec<T>
where
    T: AnchorDeserialize + Discriminator,
{
    metadata
        .logs
        .iter()
        .filter_map(|log| log.strip_prefix("Program data: "))
        .filter_map(|encoded| {
            let raw = STANDARD.decode(encoded).ok()?;
            if !raw.starts_with(T::DISCRIMINATOR) {
                return None;
            }
            let mut slice: &[u8] = &raw[T::DISCRIMINATOR.len()..];
            T::deserialize(&mut slice).ok()
        })
        .collect()
}

pub fn assert_single_event<T>(metadata: &TransactionMetadata) -> T
where
    T: AnchorDeserialize + Discriminator,
{
    let events = decode_event::<T>(metadata);
    assert_eq!(
        events.len(),
        1,
        "expected exactly one {} event, logs={:?}",
        type_name::<T>(),
        metadata.logs,
    );
    events.into_iter().next().expect("single event")
}

pub fn assert_no_event<T>(metadata: &TransactionMetadata)
where
    T: AnchorDeserialize + Discriminator,
{
    let events = decode_event::<T>(metadata);
    assert!(
        events.is_empty(),
        "expected no {} events, logs={:?}",
        type_name::<T>(),
        metadata.logs,
    );
}

pub fn assert_custom_error(failure: &FailedTransactionMetadata, code: u32) {
    let err = format!("{:?}", failure.err);
    let expected = [code, code + 6000];
    assert!(
        expected.iter().any(|value| {
            let needle = format!("Custom({value})");
            err.contains(&needle) || failure.meta.logs.iter().any(|log| log.contains(&needle))
        }),
        "expected one of {:?}, got err={err}, logs={:?}",
        expected,
        failure.meta.logs
    );
}

pub fn assert_upgrade_events(
    metadata: &TransactionMetadata,
    mandate: anchor_lang::prelude::Pubkey,
    old_plan: anchor_lang::prelude::Pubkey,
    new_plan: anchor_lang::prelude::Pubkey,
    proration_amount: i64,
) {
    let initiated = assert_single_event::<MandateUpgradeInitiated>(metadata);
    let finalized = assert_single_event::<MandateUpgradeFinalized>(metadata);
    assert_eq!(initiated.mandate, mandate);
    assert_eq!(initiated.old_plan, old_plan);
    assert_eq!(initiated.new_plan, new_plan);
    assert_eq!(initiated.proration_amount, proration_amount);
    assert_eq!(finalized.mandate, mandate);
    assert_eq!(finalized.old_plan, old_plan);
    assert_eq!(finalized.new_plan, new_plan);
    assert_eq!(finalized.proration_amount, proration_amount);
}

pub fn assert_cancel_event(
    metadata: &TransactionMetadata,
    mandate: anchor_lang::prelude::Pubkey,
    cancelled_plan: anchor_lang::prelude::Pubkey,
) {
    let cancelled = assert_single_event::<MandateUpgradeCancelled>(metadata);
    assert_eq!(cancelled.mandate, mandate);
    assert_eq!(cancelled.cancelled_plan, cancelled_plan);
}
