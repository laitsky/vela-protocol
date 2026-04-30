#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::{SubscriptionFixture, TestHarness};
use solana_program_pack::Pack;
use solana_signer::Signer;
use spl_token::state::Account as SplTokenAccount;
use vela_protocol::state::{VelaMandate, VelaPlan};

const ONE_DAY_SECONDS: u64 = 86_400;

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
    let fixture = harness.subscribe_fixture(25_000_000, ONE_DAY_SECONDS, 0, 4);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);

    let admin = harness.merchant.insecure_clone();
    let wrapped_mint_pubkey = fixture.wrapped_usdc_mint;
    let wrapping_vault = fixture.wrapping_vault;
    let vault_data = harness.fetch_account_data(&wrapping_vault);
    let vault_account =
        SplTokenAccount::unpack_from_slice(&vault_data).expect("wrapping vault should unpack");
    let spl_usdc_mint = Pubkey::new_from_array(vault_account.mint.to_bytes());

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
            plan.amount * 4,
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

fn execute_pull_at(
    harness: &mut TestHarness,
    fixture: &SubscriptionFixture,
    plan: &VelaPlan,
    due_timestamp: i64,
    subscriber_wrapped: &Pubkey,
    merchant_wrapped: &Pubkey,
    wrapped_mint: &Pubkey,
) -> VelaMandate {
    let mut mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    mandate.next_payment_due = due_timestamp;
    harness.overwrite_anchor_account(&fixture.mandate, &mandate);

    harness.set_clock_timestamp(due_timestamp);
    assert_eq!(harness.current_timestamp(), due_timestamp);

    harness.create_pull_approval_with_amount(&fixture.mandate, due_timestamp, true, plan.amount);

    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    harness
        .send_execute_pull(
            &fixture.subscriber,
            &subscriber,
            plan.plan_id,
            subscriber_wrapped,
            merchant_wrapped,
            wrapped_mint,
        )
        .expect("execute_pull should succeed at the warped timestamp");

    harness.fetch_anchor_account(&fixture.mandate)
}

#[test]
fn dst_spring_forward_keeps_next_payment_due_on_utc_seconds() {
    let (mut harness, fixture, plan, _mandate, subscriber_wrapped, merchant_wrapped, wrapped_mint) =
        setup_fixture();

    // 2026-03-08 06:59:00 UTC, one minute before the US spring-forward boundary at 07:00 UTC.
    let due_timestamp = 1_772_953_140_i64;
    let mandate_after = execute_pull_at(
        &mut harness,
        &fixture,
        &plan,
        due_timestamp,
        &subscriber_wrapped,
        &merchant_wrapped,
        &wrapped_mint,
    );

    assert_eq!(
        mandate_after.next_payment_due,
        due_timestamp + ONE_DAY_SECONDS as i64
    );
    assert_eq!(mandate_after.next_payment_due, 1_773_039_540_i64);
}

#[test]
fn utc_midnight_boundary_keeps_next_payment_due_monotonic() {
    let (mut harness, fixture, plan, _mandate, subscriber_wrapped, merchant_wrapped, wrapped_mint) =
        setup_fixture();

    // 2026-03-31 23:58:00 UTC, a few minutes before 2026-04-01 00:00:00 UTC.
    let due_timestamp = 1_775_001_480_i64;
    let mandate_after = execute_pull_at(
        &mut harness,
        &fixture,
        &plan,
        due_timestamp,
        &subscriber_wrapped,
        &merchant_wrapped,
        &wrapped_mint,
    );

    assert_eq!(
        mandate_after.next_payment_due,
        due_timestamp + ONE_DAY_SECONDS as i64
    );
    assert_eq!(mandate_after.next_payment_due, 1_775_087_880_i64);
}
