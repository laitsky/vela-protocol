#![allow(clippy::result_large_err, clippy::too_many_arguments)]

#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::prelude::Pubkey;
use helpers::{SubscriptionFixture, TestHarness};
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{BillingRail, ProtocolConfig, TokenConfig, VelaMandate, VelaPlan},
};

struct HookMintFixture {
    mint: Pubkey,
    wrapping_vault: Pubkey,
    source_token: Pubkey,
    destination_token: Pubkey,
    token_config: Pubkey,
}

fn setup_subscription_fixture() -> (
    TestHarness,
    solana_keypair::Keypair,
    Pubkey,
    SubscriptionFixture,
    VelaPlan,
    VelaMandate,
) {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let protocol_config = harness.init_protocol_config(&admin);
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    let mandate: VelaMandate = harness.fetch_anchor_account(&fixture.mandate);

    (harness, admin, protocol_config, fixture, plan, mandate)
}

fn create_hook_mint_fixture(
    harness: &mut TestHarness,
    admin: &solana_keypair::Keypair,
    owner: &Pubkey,
    destination_owner: &Pubkey,
    wrapping_vault: &Pubkey,
    initialize_wrapped_mint: bool,
) -> HookMintFixture {
    let (mint, mint_wrapping_vault) = if initialize_wrapped_mint {
        let backing_mint = harness.create_spl_mint(admin, 6);
        let wrapped_mint = Keypair::new();
        let (mint, wrapping_vault) = harness.init_wrapped_mint(admin, &wrapped_mint, &backing_mint);
        (mint, wrapping_vault)
    } else {
        (harness.create_spl_mint(admin, 6), *wrapping_vault)
    };
    harness.init_token_config(admin, &mint, BillingRail::TransferHook, 6);
    harness.init_extra_account_meta_list(admin, &mint, &mint_wrapping_vault);
    let (source_token, destination_token) = if initialize_wrapped_mint {
        (
            harness.create_token_2022_ata(admin, owner, &mint),
            harness.create_token_2022_ata(admin, destination_owner, &mint),
        )
    } else {
        (
            harness.create_spl_token_account(admin, &mint, owner),
            harness.create_spl_token_account(admin, &mint, destination_owner),
        )
    };
    let token_config = harness.derive_token_config_address(&mint);

    HookMintFixture {
        mint,
        wrapping_vault: mint_wrapping_vault,
        source_token,
        destination_token,
        token_config,
    }
}

fn call_transfer_hook_directly(
    harness: &mut TestHarness,
    source_token: &Pubkey,
    mint: &Pubkey,
    destination_token: &Pubkey,
    owner: &Pubkey,
    wrapping_vault: &Pubkey,
    config: &Pubkey,
    pull_approval: &Pubkey,
    token_config: &Pubkey,
    amount: u64,
    caller: &solana_keypair::Keypair,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let (extra_account_meta_list, _) = harness.derive_extra_account_meta_list(mint);
    let discriminator: [u8; 8] = [105, 37, 101, 197, 75, 251, 102, 26];
    let mut data = discriminator.to_vec();
    data.extend_from_slice(&amount.to_le_bytes());

    let instruction = Instruction {
        program_id: harness.hook_program_id,
        accounts: vec![
            AccountMeta::new_readonly(helpers::to_address(*source_token), false),
            AccountMeta::new_readonly(helpers::to_address(*mint), false),
            AccountMeta::new_readonly(helpers::to_address(*destination_token), false),
            AccountMeta::new_readonly(helpers::to_address(*owner), false),
            AccountMeta::new_readonly(helpers::to_address(extra_account_meta_list), false),
            AccountMeta::new_readonly(helpers::to_address(vela_protocol::ID), false),
            AccountMeta::new_readonly(helpers::to_address(*wrapping_vault), false),
            AccountMeta::new_readonly(helpers::to_address(*config), false),
            AccountMeta::new(helpers::to_address(*pull_approval), false),
            AccountMeta::new_readonly(helpers::to_address(*token_config), false),
            AccountMeta::new_readonly(helpers::to_address(anchor_lang::system_program::ID), false),
            AccountMeta::new_readonly(helpers::to_address(anchor_lang::system_program::ID), false),
            AccountMeta::new_readonly(helpers::to_address(anchor_lang::system_program::ID), false),
        ],
        data,
    };

    harness.send_instructions(&[instruction], &[caller], Some(&caller.pubkey()))
}

#[test]
fn test_multi_mint_hook_validation() {
    let (mut harness, admin, protocol_config, fixture, plan, mandate) =
        setup_subscription_fixture();
    let merchant = harness.merchant_pubkey();

    let mint_a = create_hook_mint_fixture(
        &mut harness,
        &admin,
        &fixture.mandate,
        &merchant,
        &Pubkey::default(),
        true,
    );
    let config: ProtocolConfig = harness.fetch_anchor_account(&protocol_config);
    let mint_b = create_hook_mint_fixture(
        &mut harness,
        &admin,
        &fixture.mandate,
        &merchant,
        &config.wrapping_vault,
        false,
    );
    assert_ne!(mint_a.mint, mint_b.mint);
    assert_ne!(mint_a.token_config, mint_b.token_config);

    assert_eq!(
        config.transfer_hook_program_id,
        Pubkey::new_from_array(vela_transfer_hook::ID.to_bytes())
    );

    let token_a: TokenConfig = harness.fetch_anchor_account(&mint_a.token_config);
    let token_b: TokenConfig = harness.fetch_anchor_account(&mint_b.token_config);
    assert_eq!(token_a.billing_rail, BillingRail::TransferHook);
    assert_eq!(token_b.billing_rail, BillingRail::TransferHook);

    harness.set_clock_timestamp(mandate.next_payment_due);
    let pull_approval = harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        plan.amount,
    );

    let caller_a = harness.create_wallet();
    call_transfer_hook_directly(
        &mut harness,
        &mint_a.source_token,
        &mint_a.mint,
        &mint_a.destination_token,
        &fixture.mandate,
        &mint_a.wrapping_vault,
        &protocol_config,
        &pull_approval,
        &mint_a.token_config,
        plan.amount,
        &caller_a,
    )
    .expect("hook should validate transfers for first registered mint");

    let caller_b = harness.create_wallet();
    call_transfer_hook_directly(
        &mut harness,
        &mint_b.source_token,
        &mint_b.mint,
        &mint_b.destination_token,
        &fixture.mandate,
        &mint_b.wrapping_vault,
        &protocol_config,
        &pull_approval,
        &mint_b.token_config,
        plan.amount,
        &caller_b,
    )
    .expect("hook should validate transfers for second registered mint");
}

#[test]
fn test_hook_disabled_token_fails() {
    let (mut harness, admin, protocol_config, fixture, plan, mandate) =
        setup_subscription_fixture();
    let merchant = harness.merchant_pubkey();
    let mint_fixture = create_hook_mint_fixture(
        &mut harness,
        &admin,
        &fixture.mandate,
        &merchant,
        &Pubkey::default(),
        true,
    );

    harness
        .update_token_config(
            &admin,
            &protocol_config,
            &mint_fixture.token_config,
            Some(false),
            None,
        )
        .expect("admin should be able to disable token config");

    harness.set_clock_timestamp(mandate.next_payment_due);
    let pull_approval = harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        plan.amount,
    );
    let caller = harness.create_wallet();

    let error = call_transfer_hook_directly(
        &mut harness,
        &mint_fixture.source_token,
        &mint_fixture.mint,
        &mint_fixture.destination_token,
        &fixture.mandate,
        &mint_fixture.wrapping_vault,
        &protocol_config,
        &pull_approval,
        &mint_fixture.token_config,
        plan.amount,
        &caller,
    )
    .expect_err("disabled token must fail transfer-hook validation");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("TokenDisabled") || error_str.contains("Custom(12502)"),
        "expected TokenDisabled error, got {:?}",
        error.err,
    );
}

#[test]
fn test_hook_rejects_stale_protocol_config_binding() {
    let (mut harness, admin, protocol_config, fixture, plan, mandate) =
        setup_subscription_fixture();
    let merchant = harness.merchant_pubkey();
    let mint_fixture = create_hook_mint_fixture(
        &mut harness,
        &admin,
        &fixture.mandate,
        &merchant,
        &Pubkey::default(),
        true,
    );

    harness.set_clock_timestamp(mandate.next_payment_due);
    let pull_approval = harness.create_pull_approval_with_amount(
        &fixture.mandate,
        mandate.next_payment_due,
        true,
        plan.amount,
    );

    let mut config: ProtocolConfig = harness.fetch_anchor_account(&protocol_config);
    config.transfer_hook_program_id = Pubkey::new_unique();
    harness.overwrite_anchor_account(&protocol_config, &config);

    let caller = harness.create_wallet();
    let error = call_transfer_hook_directly(
        &mut harness,
        &mint_fixture.source_token,
        &mint_fixture.mint,
        &mint_fixture.destination_token,
        &fixture.mandate,
        &mint_fixture.wrapping_vault,
        &protocol_config,
        &pull_approval,
        &mint_fixture.token_config,
        plan.amount,
        &caller,
    )
    .expect_err("stale hook binding must fail closed");

    let error_str = format!("{:?}", error.err);
    assert!(
        error_str.contains("InvalidProtocolConfig") || error_str.contains("Custom(6022)"),
        "expected InvalidProtocolConfig for stale hook binding, got {:?}",
        error.err,
    );
}
