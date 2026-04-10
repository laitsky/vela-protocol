#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::{InstructionData, ToAccountMetas};
use helpers::{to_anchor_pubkey, AgentMandateFixture, TestHarness};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::state::{AgentMandate, AgentMandateStatus};

fn send_create_agent_mandate(
    harness: &mut TestHarness,
    fixture: &AgentMandateFixture,
    daily_limit: u64,
    lifetime_cap: u64,
    min_pull_amount: u64,
    min_pull_interval: i64,
    services: Vec<vela_protocol::instructions::ServiceLimitInput>,
    funded_amount: u64,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let accounts = vela_protocol::accounts::CreateAgentMandate {
        authority: to_anchor_pubkey(fixture.authority.pubkey()),
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        agent_mandate: fixture.agent_mandate,
        authority_usdc_account: fixture.authority_spl_usdc_ata,
        mandate_wrapped_account: fixture.agent_mandate_wrapped_ata,
        wrapped_usdc_mint: fixture.wrapped_usdc_mint,
        protocol_config: config,
        spl_usdc_mint: fixture.spl_usdc_mint,
        wrapping_vault: fixture.wrapping_vault,
        mint_authority,
        spl_token_program: helpers::to_anchor_pubkey(helpers::spl_token_address()),
        token_2022_program: helpers::to_anchor_pubkey(helpers::token_2022_address()),
        associated_token_program: anchor_lang::prelude::Pubkey::new_from_array(
            spl_associated_token_account::id().to_bytes(),
        ),
        system_program: anchor_lang::system_program::ID,
    };
    let instruction = Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::CreateAgentMandate {
            daily_limit,
            lifetime_cap,
            min_pull_amount,
            min_pull_interval,
            services,
            funded_amount,
        }
        .data(),
    };

    harness.send_instructions(
        &[instruction],
        &[&fixture.authority],
        Some(&fixture.authority.pubkey()),
    )
}

fn send_create_agent_mandate_with_authority(
    harness: &mut TestHarness,
    fixture: &AgentMandateFixture,
    authority: anchor_lang::prelude::Pubkey,
    signer: &Keypair,
    funded_amount: u64,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let service = Keypair::new();
    let (agent_mandate, _) = anchor_lang::prelude::Pubkey::find_program_address(
        &[
            vela_protocol::constants::AGENT_MANDATE_SEED,
            authority.as_ref(),
            to_anchor_pubkey(fixture.agent.pubkey()).as_ref(),
        ],
        &vela_protocol::ID,
    );
    let mandate_wrapped_account =
        harness.derive_agent_mandate_wrapped_ata(&agent_mandate, &fixture.wrapped_usdc_mint);
    let accounts = vela_protocol::accounts::CreateAgentMandate {
        authority,
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        agent_mandate,
        authority_usdc_account: fixture.authority_spl_usdc_ata,
        mandate_wrapped_account,
        wrapped_usdc_mint: fixture.wrapped_usdc_mint,
        protocol_config: config,
        spl_usdc_mint: fixture.spl_usdc_mint,
        wrapping_vault: fixture.wrapping_vault,
        mint_authority,
        spl_token_program: helpers::to_anchor_pubkey(helpers::spl_token_address()),
        token_2022_program: helpers::to_anchor_pubkey(helpers::token_2022_address()),
        associated_token_program: anchor_lang::prelude::Pubkey::new_from_array(
            spl_associated_token_account::id().to_bytes(),
        ),
        system_program: anchor_lang::system_program::ID,
    };
    let instruction = Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::CreateAgentMandate {
            daily_limit: 5_000_000,
            lifetime_cap: 20_000_000,
            min_pull_amount: 100_000,
            min_pull_interval: 0,
            services: vec![vela_protocol::instructions::ServiceLimitInput {
                service: to_anchor_pubkey(service.pubkey()),
                daily_limit: 4_000_000,
            }],
            funded_amount,
        }
        .data(),
    };
    harness.send_instructions(&[instruction], &[signer], Some(&signer.pubkey()))
}

fn setup_fixture(initial_usdc_amount: u64) -> (TestHarness, AgentMandateFixture) {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, initial_usdc_amount);
    (harness, fixture)
}

#[test]
fn test_create_agent_mandate_success() {
    let (mut harness, fixture) = setup_fixture(5_000_000);
    let service = Keypair::new();
    let funded_amount = 2_500_000;

    send_create_agent_mandate(
        &mut harness,
        &fixture,
        3_000_000,
        10_000_000,
        100_000,
        60,
        vec![vela_protocol::instructions::ServiceLimitInput {
            service: to_anchor_pubkey(service.pubkey()),
            daily_limit: 2_000_000,
        }],
        funded_amount,
    )
    .expect("create_agent_mandate should succeed");

    let mandate: AgentMandate = harness.fetch_anchor_account(&fixture.agent_mandate);
    assert_eq!(mandate.authority, to_anchor_pubkey(fixture.authority.pubkey()));
    assert_eq!(mandate.agent, to_anchor_pubkey(fixture.agent.pubkey()));
    assert_eq!(mandate.daily_limit, 3_000_000);
    assert_eq!(mandate.lifetime_cap, 10_000_000);
    assert_eq!(mandate.daily_spent, 0);
    assert_eq!(mandate.total_spent, 0);
    assert_eq!(mandate.min_pull_amount, 100_000);
    assert_eq!(mandate.min_pull_interval, 60);
    assert_eq!(mandate.last_pull_at, 0);
    assert!(matches!(mandate.status, AgentMandateStatus::Active));
    assert_eq!(mandate.services.len(), 1);
    assert_eq!(mandate.services[0].service, to_anchor_pubkey(service.pubkey()));
    assert_eq!(mandate.services[0].daily_limit, 2_000_000);
    assert_eq!(mandate.services[0].daily_spent, 0);
    assert_eq!(mandate.services[0].last_reset, mandate.daily_last_reset);

    let wrapped_balance = harness.fetch_spl_token_account(&fixture.agent_mandate_wrapped_ata);
    assert_eq!(wrapped_balance.amount, funded_amount);

    let authority_spl_balance = harness.fetch_spl_token_account(&fixture.authority_spl_usdc_ata);
    assert_eq!(authority_spl_balance.amount, 5_000_000 - funded_amount);
    let wrapping_vault_balance = harness.fetch_spl_token_account(&fixture.wrapping_vault);
    assert_eq!(wrapping_vault_balance.amount, funded_amount);

    let (expected_pda, expected_bump) = anchor_lang::prelude::Pubkey::find_program_address(
        &[
            vela_protocol::constants::AGENT_MANDATE_SEED,
            to_anchor_pubkey(fixture.authority.pubkey()).as_ref(),
            to_anchor_pubkey(fixture.agent.pubkey()).as_ref(),
        ],
        &vela_protocol::ID,
    );
    assert_eq!(expected_pda, fixture.agent_mandate);
    assert_eq!(expected_bump, mandate.bump);
}

#[test]
fn test_agent_mandate_create_success_wrapper() {
    test_create_agent_mandate_success();
}

#[test]
fn test_create_agent_mandate_rejects_duplicate_services() {
    let (mut harness, fixture) = setup_fixture(5_000_000);
    let service = Keypair::new();
    let service_key = to_anchor_pubkey(service.pubkey());

    let error = send_create_agent_mandate(
        &mut harness,
        &fixture,
        3_000_000,
        10_000_000,
        100_000,
        60,
        vec![
            vela_protocol::instructions::ServiceLimitInput {
                service: service_key,
                daily_limit: 1_000_000,
            },
            vela_protocol::instructions::ServiceLimitInput {
                service: service_key,
                daily_limit: 2_000_000,
            },
        ],
        2_500_000,
    )
    .expect_err("duplicate services should be rejected");

    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected duplicate service custom error, got {:?}",
        error.err,
    );
    assert!(
        harness
            .svm
            .get_account(&helpers::to_address(fixture.agent_mandate))
            .is_none(),
        "mandate account should not be created on duplicate services",
    );
}

#[test]
fn test_create_agent_mandate_rejects_negative_min_pull_interval() {
    let (mut harness, fixture) = setup_fixture(5_000_000);
    let service = Keypair::new();

    let error = send_create_agent_mandate(
        &mut harness,
        &fixture,
        3_000_000,
        10_000_000,
        100_000,
        -1,
        vec![vela_protocol::instructions::ServiceLimitInput {
            service: to_anchor_pubkey(service.pubkey()),
            daily_limit: 2_000_000,
        }],
        2_500_000,
    )
    .expect_err("negative min_pull_interval should fail");

    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_agent_mandate_create_rejects_duplicate_services_wrapper() {
    test_create_agent_mandate_rejects_duplicate_services();
}

#[test]
fn test_create_agent_mandate_rejects_more_than_max_services() {
    let (mut harness, fixture) = setup_fixture(5_000_000);
    let mut services = Vec::new();
    for _ in 0..9 {
        services.push(vela_protocol::instructions::ServiceLimitInput {
            service: to_anchor_pubkey(Keypair::new().pubkey()),
            daily_limit: 1_000_000,
        });
    }

    let error = send_create_agent_mandate(
        &mut harness,
        &fixture,
        3_000_000,
        10_000_000,
        100_000,
        60,
        services,
        2_500_000,
    )
    .expect_err("service list above maximum should be rejected");

    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected too-many-services custom error, got {:?}",
        error.err,
    );
    assert!(
        harness
            .svm
            .get_account(&helpers::to_address(fixture.agent_mandate))
            .is_none(),
        "mandate account should not be created when services exceed max",
    );
}

#[test]
fn test_agent_mandate_create_rejects_more_than_max_services_wrapper() {
    test_create_agent_mandate_rejects_more_than_max_services();
}

#[test]
fn test_non_authority_create_agent_mandate_fails() {
    let (mut harness, fixture) = setup_fixture(5_000_000);
    let intruder = harness.create_wallet();
    let authority = to_anchor_pubkey(fixture.authority.pubkey());
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        send_create_agent_mandate_with_authority(
            &mut harness,
            &fixture,
            authority,
            &intruder,
            2_500_000,
        )
    }));

    match attempt {
        Ok(result) => {
            let error =
                result.expect_err("non-authority signer should not be able to create mandate for authority");
            let err = format!("{:?}", error.err);
            assert!(
                err.contains("MissingRequiredSignature") || err.contains("Custom("),
                "expected authority/signature enforcement failure, got {:?}",
                error.err,
            );
        }
        Err(panic) => {
            let panic_text = if let Some(text) = panic.downcast_ref::<String>() {
                text.clone()
            } else if let Some(text) = panic.downcast_ref::<&str>() {
                text.to_string()
            } else {
                String::new()
            };
            assert!(
                panic_text.contains("NotEnoughSigners"),
                "expected signer enforcement panic, got: {panic_text}",
            );
        }
    }
}
