#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use solana_signer::Signer;
use upgrade_helpers::{
    assert_custom_error, fetch_mandate, setup_periodic_upgrade_fixture, PERIOD_SECONDS,
};

#[test]
fn test_higher_price_upgrade_requires_subscriber_authority_and_preserves_credential_address() {
    let mut fixture = setup_periodic_upgrade_fixture(10_000_000, 20_000_000);
    let half_period = (PERIOD_SECONDS / 2) as i64;
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due - half_period);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        10_000_000,
    );

    let merchant = fixture.harness.merchant.insecure_clone();
    let credential_before = fixture.harness.derive_credential_ata(
        &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
        &fixture.plan_a_state.credential_mint,
    );

    let error = fixture
        .harness
        .send_update_mandate_plan(
            &merchant,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect_err("merchant-only upgrade above current amount must fail");
    assert_custom_error(&error, 6716);

    fixture
        .harness
        .send_update_mandate_plan(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.wrapped_mint,
        )
        .expect("subscriber should authorize higher-price upgrade");

    let mandate_after = fetch_mandate(&fixture.harness, &fixture.mandate);
    assert_eq!(mandate_after.plan, fixture.plan_b);

    let credential_after = fixture.harness.derive_credential_ata(
        &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
        &fixture.plan_b_state.credential_mint,
    );
    assert_eq!(credential_before, credential_after);
}

#[test]
fn test_update_mandate_plan_rejects_wrong_wrapped_mint_as_token_change() {
    let mut fixture = setup_periodic_upgrade_fixture(10_000_000, 20_000_000);
    let half_period = (PERIOD_SECONDS / 2) as i64;
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    fixture
        .harness
        .set_clock_timestamp(mandate_before.next_payment_due - half_period);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        10_000_000,
    );

    let error = fixture
        .harness
        .send_update_mandate_plan_with_mint(
            &fixture.subscriber,
            &fixture.mandate,
            &fixture.plan_b,
            &fixture.subscriber_wrapped,
            &fixture.merchant_wrapped,
            &fixture.spl_usdc_mint,
        )
        .expect_err("wrong mint should be rejected as token change");
    assert_custom_error(&error, 6712);
}
