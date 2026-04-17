#![allow(dead_code)]

#[path = "helpers/mod.rs"]
pub mod helpers;

use std::any::type_name;

use anchor_lang::__private::base64::{engine::general_purpose::STANDARD, Engine as _};
use anchor_lang::{AnchorDeserialize, Discriminator, InstructionData, ToAccountMetas};
use helpers::{convert_account_meta, TestHarness};
use litesvm::types::{FailedTransactionMetadata, TransactionMetadata};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_program_pack::Pack;
use solana_signer::Signer;
use spl_token_2022::state::Account as Token2022Account;
use vela_protocol::state::{
    MandateUpgradeCancelled, MandateUpgradeFinalized, MandateUpgradeInitiated, VelaMandate,
    VelaPlan,
};

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

pub fn setup_periodic_upgrade_fixture(amount_a: u64, amount_b: u64) -> PeriodicUpgradeFixture {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let subscriber_pubkey =
        anchor_lang::prelude::Pubkey::new_from_array(subscriber.pubkey().to_bytes());

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

    let subscriber_wrapped = harness.create_token_2022_ata(&admin, &mandate, &wrapped_mint_pubkey);
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

pub fn wrapped_balance(harness: &TestHarness, account: &anchor_lang::prelude::Pubkey) -> u64 {
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
    old_plan: anchor_lang::prelude::Pubkey,
    new_plan: anchor_lang::prelude::Pubkey,
) {
    let cancelled = assert_single_event::<MandateUpgradeCancelled>(metadata);
    assert_eq!(cancelled.mandate, mandate);
    assert_eq!(cancelled.old_plan, old_plan);
    assert_eq!(cancelled.new_plan, new_plan);
    assert_eq!(cancelled.proration_amount, 0);
    assert_eq!(cancelled.change_type, 2);
}

pub fn send_periodic_execute_pull(
    fixture: &mut PeriodicUpgradeFixture,
    payer: &Keypair,
    plan: &anchor_lang::prelude::Pubkey,
    pending_plan: Option<&anchor_lang::prelude::Pubkey>,
) -> Result<TransactionMetadata, FailedTransactionMetadata> {
    let subscriber =
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes());
    let config = fixture.harness.derive_config();
    let config_account: vela_protocol::state::ProtocolConfig =
        fixture.harness.fetch_anchor_account(&config);
    let (extra_account_meta_list, _) = fixture
        .harness
        .derive_extra_account_meta_list(&fixture.wrapped_mint);
    let keeper_config = fixture.harness.ensure_keeper_config(payer);

    let accounts = vela_protocol::accounts::ExecutePull {
        payer: anchor_lang::prelude::Pubkey::new_from_array(payer.pubkey().to_bytes()),
        subscriber,
        merchant: fixture.harness.merchant_pubkey(),
        keeper_config,
        plan: *plan,
        mandate: fixture.mandate,
        subscriber_wrapped_account: fixture.subscriber_wrapped,
        merchant_wrapped_account: fixture.merchant_wrapped,
        wrapped_usdc_mint: fixture.wrapped_mint,
        pull_approval: fixture
            .harness
            .derive_pull_approval_address(&fixture.mandate),
        token_config: fixture
            .harness
            .derive_token_config_address(&fixture.wrapped_mint),
        protocol_config: config,
        wrapping_vault: config_account.wrapping_vault,
        hook_program: anchor_lang::prelude::Pubkey::new_from_array(
            vela_transfer_hook::ID.to_bytes(),
        ),
        extra_account_meta_list,
        protocol_program: vela_protocol::ID,
        token_2022_program: anchor_spl::token_2022::ID,
        system_program: anchor_lang::system_program::ID,
    };

    let mut metas = accounts
        .to_account_metas(None)
        .into_iter()
        .map(convert_account_meta)
        .collect::<Vec<_>>();

    if let Some(pending_plan) = pending_plan {
        metas.push(convert_account_meta(
            anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
                *pending_plan,
                false,
            ),
        ));
    }

    let instruction = Instruction {
        program_id: fixture.harness.program_id,
        accounts: metas,
        data: vela_protocol::instruction::ExecutePull {}.data(),
    };

    fixture
        .harness
        .send_instructions(&[instruction], &[payer], Some(&payer.pubkey()))
}
