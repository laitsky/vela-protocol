#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use upgrade_helpers::{
    assert_single_event, assert_upgrade_events, fetch_mandate, send_periodic_execute_pull,
    setup_periodic_upgrade_fixture, wrapped_balance,
};
use vela_protocol::constants::WRAPPED_USDC_SYMBOL;
use vela_protocol::state::{MandateUpgradeFinalized, MandateUpgradeInitiated};

const BASIC: u64 = 10_000_000;
const PREMIUM: u64 = 20_000_000;

#[test]
fn wrapped_usdc_mandates_still_pull_and_upgrade_on_same_mint() {
    let mut fixture = setup_periodic_upgrade_fixture(BASIC, PREMIUM);
    let mut mandate = fetch_mandate(&fixture.harness, &fixture.mandate);

    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due + 600,
        true,
        BASIC,
    );
    fixture
        .harness
        .set_clock_timestamp(mandate.next_payment_due);

    let payer = fixture.subscriber.insecure_clone();
    let plan_a = fixture.plan_a;
    let plan_b = fixture.plan_b;
    let subscriber_before = wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped);
    let merchant_before = wrapped_balance(&fixture.harness, &fixture.merchant_wrapped);

    send_periodic_execute_pull(&mut fixture, &payer, &plan_a, None)
        .expect("existing USDC mandate pull should succeed");

    mandate = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate.plan, fixture.plan_a);
    assert_eq!(mandate.amount, BASIC);
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.subscriber_wrapped),
        subscriber_before - BASIC,
    );
    assert_eq!(
        wrapped_balance(&fixture.harness, &fixture.merchant_wrapped),
        merchant_before + BASIC,
    );

    let halfway = mandate.last_pull_at + ((mandate.next_payment_due - mandate.last_pull_at) / 2);
    fixture.harness.set_clock_timestamp(halfway);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due + 600,
        true,
        BASIC,
    );

    let metadata = fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("same-mint upgrade should still succeed");

    assert_upgrade_events(&metadata, fixture.mandate, plan_a, plan_b, 5_000_000);
    let initiated = assert_single_event::<MandateUpgradeInitiated>(&metadata);
    let finalized = assert_single_event::<MandateUpgradeFinalized>(&metadata);
    assert_eq!(initiated.mint, fixture.wrapped_mint);
    assert_eq!(initiated.token_symbol, WRAPPED_USDC_SYMBOL);
    assert_eq!(finalized.mint, fixture.wrapped_mint);
    assert_eq!(finalized.token_symbol, WRAPPED_USDC_SYMBOL);

    mandate = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate.plan, fixture.plan_b);
    assert_eq!(mandate.amount, PREMIUM);

    assert_eq!(
        mandate.pending_new_plan,
        anchor_lang::prelude::Pubkey::default()
    );
    assert_eq!(mandate.pending_effective_at, 0);
    assert_eq!(mandate.pending_change_type, 0);
}
