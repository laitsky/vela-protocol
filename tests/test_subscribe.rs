#[path = "helpers/mod.rs"]
mod helpers;

use helpers::TestHarness;
use solana_program_pack::Pack;
use solana_signer::Signer;
use spl_token_2022::state::Account as Token2022Account;
use vela_protocol::{
    constants::{MIN_FREQUENCY_SECONDS, USDC_DECIMALS},
    state::{MandateStatus, PlanStatus, VelaMandate, VelaPlan},
};

fn setup_subscription_fixture() -> (
    TestHarness,
    solana_keypair::Keypair,
    vela_protocol::state::VelaPlan,
    Pubkey,
    Pubkey,
    Pubkey,
) {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let amount = 25_000_000;
    let max_pulls = 4;
    let frequency = MIN_FREQUENCY_SECONDS;
    let addresses = harness.derive_plan_addresses(0);

    harness
        .send_create_plan(amount, frequency, 0, max_pulls, 0)
        .expect("create_plan should succeed");

    let usdc_mint = harness.create_spl_mint(&subscriber, USDC_DECIMALS);
    let subscriber_token_account = harness.create_spl_token_account(
        &subscriber,
        &usdc_mint,
        &Pubkey::new_from_array(subscriber.pubkey().to_bytes()),
    );
    harness.mint_spl_tokens(&subscriber, &usdc_mint, &subscriber_token_account, amount * max_pulls + amount);

    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);
    (
        harness,
        subscriber,
        plan,
        addresses.plan,
        addresses.credential_mint,
        subscriber_token_account,
    )
}

use anchor_lang::prelude::Pubkey;

#[test]
fn test_subscribe_success() {
    let (mut harness, subscriber, plan, plan_address, credential_mint, subscriber_token_account) =
        setup_subscription_fixture();
    let now = harness.current_timestamp();
    let usdc_mint = harness.fetch_spl_token_account(&subscriber_token_account).mint;
    let usdc_mint = Pubkey::new_from_array(usdc_mint.to_bytes());

    let meta = harness
        .send_subscribe(&subscriber, plan.plan_id, &subscriber_token_account, &usdc_mint)
        .expect("subscribe should succeed");

    assert!(meta.compute_units_consumed > 0);

    let mandate_address = harness.derive_mandate_address(
        &Pubkey::new_from_array(subscriber.pubkey().to_bytes()),
        &plan_address,
    );
    let mandate: VelaMandate = harness.fetch_anchor_account(&mandate_address);
    assert_eq!(mandate.subscriber, Pubkey::new_from_array(subscriber.pubkey().to_bytes()));
    assert_eq!(mandate.plan, plan_address);
    assert_eq!(mandate.merchant, harness.merchant_pubkey());
    assert_eq!(mandate.amount, plan.amount);
    assert_eq!(mandate.frequency, plan.frequency);
    assert_eq!(mandate.start_date, now);
    assert_eq!(mandate.max_pulls, plan.max_pulls);
    assert_eq!(mandate.pulls_executed, 0);
    assert_eq!(mandate.next_payment_due, now + plan.frequency as i64);
    assert!(matches!(mandate.status, MandateStatus::Active));

    let subscriber_token = harness.fetch_spl_token_account(&subscriber_token_account);
    assert_eq!(
        subscriber_token
            .delegate
            .expect("delegate should be set")
            .to_string(),
        mandate_address.to_string()
    );
    assert_eq!(subscriber_token.delegated_amount, plan.amount * plan.max_pulls);

    let credential_ata = harness.derive_credential_ata(
        &Pubkey::new_from_array(subscriber.pubkey().to_bytes()),
        &credential_mint,
    );
    let credential_account =
        Token2022Account::unpack_from_slice(&harness.fetch_account_data(&credential_ata))
            .expect("credential account should unpack");
    assert_eq!(credential_account.mint.to_string(), credential_mint.to_string());
    assert_eq!(
        credential_account.owner.to_string(),
        Pubkey::new_from_array(subscriber.pubkey().to_bytes()).to_string()
    );
    assert_eq!(credential_account.amount, 1);
}

#[test]
fn test_subscribe_trial_period_extends_expiry_from_first_bill() {
    let mut harness = TestHarness::new();
    let subscriber = harness.create_wallet();
    let amount = 25_000_000;
    let max_pulls = 4;
    let frequency = MIN_FREQUENCY_SECONDS;
    let trial_period = MIN_FREQUENCY_SECONDS * 2;
    let addresses = harness.derive_plan_addresses(0);

    harness
        .send_create_plan(amount, frequency, trial_period, max_pulls, 0)
        .expect("create_plan should succeed");

    let usdc_mint = harness.create_spl_mint(&subscriber, USDC_DECIMALS);
    let subscriber_token_account = harness.create_spl_token_account(
        &subscriber,
        &usdc_mint,
        &Pubkey::new_from_array(subscriber.pubkey().to_bytes()),
    );
    harness.mint_spl_tokens(&subscriber, &usdc_mint, &subscriber_token_account, amount * max_pulls + amount);

    let now = harness.current_timestamp();
    let plan: VelaPlan = harness.fetch_anchor_account(&addresses.plan);

    harness
        .send_subscribe(&subscriber, plan.plan_id, &subscriber_token_account, &usdc_mint)
        .expect("subscribe should succeed");

    let mandate_address = harness.derive_mandate_address(
        &Pubkey::new_from_array(subscriber.pubkey().to_bytes()),
        &addresses.plan,
    );
    let mandate: VelaMandate = harness.fetch_anchor_account(&mandate_address);

    assert_eq!(mandate.next_payment_due, now + trial_period as i64);
    assert_eq!(
        mandate.expiry,
        now + trial_period as i64 + (plan.frequency * plan.max_pulls) as i64
    );
}

#[test]
fn test_subscribe_inactive_plan() {
    let (mut harness, subscriber, mut plan, plan_address, _credential_mint, subscriber_token_account) =
        setup_subscription_fixture();
    let usdc_mint = harness.fetch_spl_token_account(&subscriber_token_account).mint;
    let usdc_mint = Pubkey::new_from_array(usdc_mint.to_bytes());
    plan.status = PlanStatus::Inactive;
    harness.overwrite_anchor_account(&plan_address, &plan);

    let error = harness
        .send_subscribe(&subscriber, plan.plan_id, &subscriber_token_account, &usdc_mint)
        .expect_err("subscribe should reject inactive plans");

    assert!(
        format!("{:?}", error.err).contains("Custom(6007)"),
        "expected PlanNotActive custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_subscribe_cu_budget() {
    let (mut harness, subscriber, plan, _plan_address, _credential_mint, subscriber_token_account) =
        setup_subscription_fixture();
    let usdc_mint = harness.fetch_spl_token_account(&subscriber_token_account).mint;
    let usdc_mint = Pubkey::new_from_array(usdc_mint.to_bytes());

    let meta = harness
        .send_subscribe(&subscriber, plan.plan_id, &subscriber_token_account, &usdc_mint)
        .expect("subscribe should succeed");

    assert!(
        meta.compute_units_consumed < 150_000,
        "subscribe CU budget exceeded: {}",
        meta.compute_units_consumed
    );
}
