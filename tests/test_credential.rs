#[path = "helpers/mod.rs"]
mod helpers;

use std::collections::BTreeMap;

use anchor_lang::prelude::Pubkey;
use helpers::TestHarness;
use solana_program_pack::Pack;
use solana_pubkey::Pubkey as SplPubkey;
use solana_signer::Signer;
use spl_token_2022::{
    extension::{BaseStateWithExtensions, StateWithExtensions},
    state::{Account as Token2022Account, Mint},
};
use spl_token_metadata_interface::state::TokenMetadata;
use vela_protocol::constants::MIN_FREQUENCY_SECONDS;
use vela_protocol::state::VelaMandate;

#[test]
fn test_credential_is_non_transferable() {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let recipient = harness.create_wallet();
    let recipient_pubkey = Pubkey::new_from_array(recipient.pubkey().to_bytes());
    let source_ata = harness.derive_credential_ata(&subscriber, &fixture.credential_mint);
    let destination_ata = harness.derive_credential_ata(&recipient_pubkey, &fixture.credential_mint);

    let create_destination_ata =
        spl_associated_token_account::instruction::create_associated_token_account_idempotent(
            &SplPubkey::new_from_array(recipient.pubkey().to_bytes()),
            &SplPubkey::new_from_array(recipient_pubkey.to_bytes()),
            &SplPubkey::new_from_array(fixture.credential_mint.to_bytes()),
            &SplPubkey::new_from_array(spl_token_2022::id().to_bytes()),
        );
    let transfer_ix = spl_token_2022::instruction::transfer_checked(
        &SplPubkey::new_from_array(spl_token_2022::id().to_bytes()),
        &SplPubkey::new_from_array(source_ata.to_bytes()),
        &SplPubkey::new_from_array(fixture.credential_mint.to_bytes()),
        &SplPubkey::new_from_array(destination_ata.to_bytes()),
        &SplPubkey::new_from_array(subscriber.to_bytes()),
        &[],
        1,
        0,
    )
    .expect("transfer instruction should build");

    let error = harness
        .send_instructions(
            &[create_destination_ata, transfer_ix],
            &[&recipient, &fixture.subscriber],
            Some(&recipient.pubkey()),
        )
        .expect_err("non-transferable credentials should not transfer");

    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected a token-2022 transfer failure, got {:?}",
        error.err,
    );
}

#[test]
fn test_credential_has_metadata() {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4);
    let mint_data = harness.fetch_account_data(&fixture.credential_mint);
    let mint = StateWithExtensions::<Mint>::unpack(&mint_data)
        .expect("credential mint should unpack with extensions");
    let metadata = mint
        .get_variable_len_extension::<TokenMetadata>()
        .expect("token metadata should be present");
    let metadata_map = metadata
        .additional_metadata
        .iter()
        .cloned()
        .collect::<BTreeMap<_, _>>();
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);
    let now = harness.current_timestamp();

    assert_eq!(metadata.name, "Vela Plan #0");
    assert_eq!(metadata.symbol, "VELA");
    assert_eq!(metadata.mint, SplPubkey::new_from_array(fixture.credential_mint.to_bytes()));
    assert_eq!(
        metadata_map.get("plan_tier").map(String::as_str),
        Some("Vela Plan #0")
    );
    assert_eq!(metadata_map.get("plan_id").map(String::as_str), Some("0"));
    assert_eq!(
        metadata_map
            .get("subscription_start_source")
            .map(String::as_str),
        Some("VelaMandate.start_date")
    );
    assert!(
        !metadata_map.contains_key("start_date"),
        "shared credential metadata should not duplicate subscriber-specific start dates"
    );
    assert_eq!(mandate.start_date, now);
    assert_eq!(mandate.next_payment_due, now + mandate.frequency as i64);
}

#[test]
fn test_credential_per_plan_mint() {
    let mut harness = TestHarness::new();
    let subscriber_one = harness.create_wallet();
    let subscriber_one_pubkey = Pubkey::new_from_array(subscriber_one.pubkey().to_bytes());
    let subscriber_two = harness.create_wallet();
    let subscriber_two_pubkey = Pubkey::new_from_array(subscriber_two.pubkey().to_bytes());

    harness
        .send_create_plan(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 0)
        .expect("first plan should succeed");
    harness
        .send_create_plan(50_000_000, MIN_FREQUENCY_SECONDS, 0, 4, 1)
        .expect("second plan should succeed");

    let plan_a = harness.derive_plan_addresses(0);
    let plan_b = harness.derive_plan_addresses(1);
    assert_ne!(plan_a.credential_mint, plan_b.credential_mint);

    // Subscribe three different subscribers/plans (no USDC accounts needed anymore)
    harness
        .send_subscribe(&subscriber_one, 0)
        .expect("first subscriber should join plan A");
    harness
        .send_subscribe(&subscriber_one, 1)
        .expect("first subscriber should join plan B");
    harness
        .send_subscribe(&subscriber_two, 0)
        .expect("second subscriber should join plan A");

    let plan_a_credential_one =
        harness.derive_credential_ata(&subscriber_one_pubkey, &plan_a.credential_mint);
    let plan_a_credential_two =
        harness.derive_credential_ata(&subscriber_two_pubkey, &plan_a.credential_mint);
    let plan_b_credential_one =
        harness.derive_credential_ata(&subscriber_one_pubkey, &plan_b.credential_mint);

    let credential_one_a =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&plan_a_credential_one))
            .expect("plan A credential should unpack");
    let credential_two_a =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&plan_a_credential_two))
            .expect("second plan A credential should unpack");
    let credential_one_b =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&plan_b_credential_one))
            .expect("plan B credential should unpack");

    assert_eq!(
        credential_one_a.mint,
        SplPubkey::new_from_array(plan_a.credential_mint.to_bytes())
    );
    assert_eq!(
        credential_two_a.mint,
        SplPubkey::new_from_array(plan_a.credential_mint.to_bytes())
    );
    assert_eq!(
        credential_one_b.mint,
        SplPubkey::new_from_array(plan_b.credential_mint.to_bytes())
    );
}

#[test]
fn test_credential_burned_on_cancel() {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 4);
    let subscriber = Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let credential_ata = harness.derive_credential_ata(&subscriber, &fixture.credential_mint);

    let before =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential should exist before cancel");
    assert_eq!(before.amount, 1);

    harness
        .send_cancel(
            &fixture.subscriber,
            &subscriber,
            0,
            &fixture.mandate,
            &fixture.subscriber_token_account,
        )
        .expect("cancel should succeed");

    let after =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential account should still exist after cancel");
    assert_eq!(after.amount, 0);
}
