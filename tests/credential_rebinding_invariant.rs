#![allow(clippy::duplicate_mod)]

#[path = "helpers/mod.rs"]
mod helpers;
#[path = "upgrade_helpers.rs"]
mod upgrade_helpers;

use helpers::TestHarness;
use solana_signer::Signer;
use upgrade_helpers::{fetch_mandate, setup_periodic_upgrade_fixture};
use vela_protocol::state::{MerchantState, StreamMandate};

fn setup_stream_fixture() -> (
    TestHarness,
    solana_keypair::Keypair,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
) {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let admin = harness.merchant.insecure_clone();
    let spl_usdc_mint = harness.create_spl_mint(&admin, 6);
    harness.init_protocol_config(&admin);

    let wrapped_mint = solana_keypair::Keypair::new();
    let (wrapped_mint_pubkey, wrapping_vault) =
        harness.init_wrapped_mint(&admin, &wrapped_mint, &spl_usdc_mint);
    harness.init_extra_account_meta_list(&admin, &wrapped_mint_pubkey, &wrapping_vault);
    harness
        .send_init_merchant_credential()
        .expect("merchant credential bootstrap should succeed");

    let merchant = harness.merchant_pubkey();
    let (merchant_state, _) = anchor_lang::prelude::Pubkey::find_program_address(
        &[MerchantState::SEED_PREFIX, merchant.as_ref()],
        &vela_protocol::ID,
    );
    let merchant_state_before: MerchantState = harness.fetch_anchor_account(&merchant_state);

    harness
        .send_create_stream_mandate(&subscriber, &wrapped_mint_pubkey, 10, 20, None, 60)
        .expect("create_stream_mandate should succeed");

    let stream_mandate = harness.derive_stream_mandate_address_by_index(
        &subscriber_pubkey,
        &merchant,
        merchant_state_before.stream_mandate_counter,
    );
    let created_stream: StreamMandate = harness.fetch_anchor_account(&stream_mandate);
    harness.create_pull_approval_with_amount(
        &stream_mandate,
        created_stream.last_settled_ts + 600,
        true,
        u64::MAX,
    );

    let subscriber_usdc =
        harness.create_spl_token_account(&subscriber, &spl_usdc_mint, &subscriber_pubkey);
    harness.mint_spl_tokens(&admin, &spl_usdc_mint, &subscriber_usdc, 10_000);
    let subscriber_wrapped =
        harness.create_token_2022_ata(&admin, &stream_mandate, &wrapped_mint_pubkey);
    harness
        .send_wrap(
            &subscriber,
            &spl_usdc_mint,
            &wrapped_mint_pubkey,
            &subscriber_usdc,
            &subscriber_wrapped,
            &stream_mandate,
            &wrapping_vault,
            10_000,
        )
        .expect("wrap into stream mandate account should succeed");
    let merchant_wrapped = harness.create_token_2022_ata(&admin, &merchant, &wrapped_mint_pubkey);

    (
        harness,
        subscriber,
        stream_mandate,
        subscriber_wrapped,
        merchant_wrapped,
        wrapped_mint_pubkey,
    )
}

#[test]
fn periodic_plan_upgrade_does_not_rebind_credential_address() {
    let mut fixture = setup_periodic_upgrade_fixture(10_000_000, 20_000_000);
    let merchant_credential_mint = fixture.plan_a_state.credential_mint;
    let subscriber =
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let credential_before = fixture
        .harness
        .derive_credential_ata(&subscriber, &merchant_credential_mint);
    let mandate_before = fetch_mandate(&fixture.harness, &fixture.mandate);
    let halfway = mandate_before.start_date
        + ((mandate_before.next_payment_due - mandate_before.start_date) / 2);

    fixture.harness.set_clock_timestamp(halfway);
    fixture.harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate_before.next_payment_due + 600,
        true,
        10_000_000,
    );
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
        .expect("upgrade should succeed");

    let credential_after = fixture
        .harness
        .derive_credential_ata(&subscriber, &fixture.plan_b_state.credential_mint);

    assert_eq!(
        merchant_credential_mint,
        fixture.plan_b_state.credential_mint
    );
    assert_eq!(credential_before, credential_after);
}

#[test]
fn stream_rate_change_does_not_rebind_credential_address() {
    let (
        mut harness,
        subscriber,
        stream_mandate,
        subscriber_wrapped,
        merchant_wrapped,
        wrapped_mint,
    ) = setup_stream_fixture();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(subscriber.pubkey().to_bytes());
    let (merchant_credential_mint, _) = harness.derive_merchant_credential_mint();
    let credential_before =
        harness.derive_credential_ata(&subscriber_pubkey, &merchant_credential_mint);

    harness
        .send_update_stream_rate(
            &subscriber,
            &stream_mandate,
            &subscriber_wrapped,
            &merchant_wrapped,
            &wrapped_mint,
            Some(15),
            Some(25),
        )
        .expect("stream rate update should succeed");

    let credential_after =
        harness.derive_credential_ata(&subscriber_pubkey, &merchant_credential_mint);
    assert_eq!(credential_before, credential_after);
}
