#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::{InstructionData, ToAccountMetas};
use helpers::{to_anchor_pubkey, AgentMandateFixture, TestHarness};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    instructions::ServiceLimitInput,
    state::{AgentMandate, AgentMandateStatus},
};

fn send_create_agent_mandate(
    harness: &mut TestHarness,
    fixture: &AgentMandateFixture,
    service: anchor_lang::prelude::Pubkey,
    funded_amount: u64,
) {
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
            daily_limit: 5_000_000,
            lifetime_cap: 20_000_000,
            min_pull_amount: 100_000,
            min_pull_interval: 0,
            services: vec![ServiceLimitInput {
                service,
                daily_limit: 4_000_000,
            }],
            funded_amount,
        }
        .data(),
    };
    harness
        .send_instructions(
            &[instruction],
            &[&fixture.authority],
            Some(&fixture.authority.pubkey()),
        )
        .expect("create_agent_mandate should succeed");
}

fn send_agent_pull(
    harness: &mut TestHarness,
    fixture: &AgentMandateFixture,
    payer: &Keypair,
    service_wrapped_account: &anchor_lang::prelude::Pubkey,
    amount: u64,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let config = harness.derive_config();
    let pull_approval = harness.derive_agent_pull_approval_address(&fixture.agent_mandate);
    let accounts = vela_protocol::accounts::AgentPull {
        payer: to_anchor_pubkey(payer.pubkey()),
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        authority: to_anchor_pubkey(fixture.authority.pubkey()),
        agent_mandate: fixture.agent_mandate,
        mandate_wrapped_account: fixture.agent_mandate_wrapped_ata,
        service_wrapped_account: *service_wrapped_account,
        pull_approval,
        wrapped_usdc_mint: fixture.wrapped_usdc_mint,
        protocol_config: config,
        wrapping_vault: fixture.wrapping_vault,
        hook_program: anchor_lang::prelude::Pubkey::new_from_array(vela_transfer_hook::ID.to_bytes()),
        extra_account_meta_list: fixture.extra_account_meta_list,
        protocol_program: vela_protocol::ID,
        token_2022_program: helpers::to_anchor_pubkey(helpers::token_2022_address()),
        system_program: anchor_lang::system_program::ID,
    };
    let instruction = Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::AgentPull { amount }.data(),
    };
    harness.send_instructions(
        &[instruction],
        &[payer, &fixture.agent],
        Some(&payer.pubkey()),
    )
}

fn send_pause_agent_mandate(
    harness: &mut TestHarness,
    authority: &Keypair,
    fixture: &AgentMandateFixture,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let accounts = vela_protocol::accounts::PauseAgentMandate {
        authority: to_anchor_pubkey(authority.pubkey()),
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        agent_mandate: fixture.agent_mandate,
    };
    let instruction = Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::PauseAgentMandate {}.data(),
    };
    harness.send_instructions(&[instruction], &[authority], Some(&authority.pubkey()))
}

fn send_resume_agent_mandate(
    harness: &mut TestHarness,
    authority: &Keypair,
    fixture: &AgentMandateFixture,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let accounts = vela_protocol::accounts::ResumeAgentMandate {
        authority: to_anchor_pubkey(authority.pubkey()),
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        agent_mandate: fixture.agent_mandate,
    };
    let instruction = Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::ResumeAgentMandate {}.data(),
    };
    harness.send_instructions(&[instruction], &[authority], Some(&authority.pubkey()))
}

fn send_revoke_agent_mandate(
    harness: &mut TestHarness,
    authority: &Keypair,
    authority_spl_usdc_ata: anchor_lang::prelude::Pubkey,
    fixture: &AgentMandateFixture,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let accounts = vela_protocol::accounts::RevokeAgentMandate {
        authority: to_anchor_pubkey(authority.pubkey()),
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        agent_mandate: fixture.agent_mandate,
        mandate_wrapped_account: fixture.agent_mandate_wrapped_ata,
        authority_usdc_account: authority_spl_usdc_ata,
        wrapped_usdc_mint: fixture.wrapped_usdc_mint,
        protocol_config: config,
        spl_usdc_mint: fixture.spl_usdc_mint,
        wrapping_vault: fixture.wrapping_vault,
        mint_authority,
        spl_token_program: helpers::to_anchor_pubkey(helpers::spl_token_address()),
        token_2022_program: helpers::to_anchor_pubkey(helpers::token_2022_address()),
    };
    let instruction = Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::RevokeAgentMandate {}.data(),
    };
    harness.send_instructions(&[instruction], &[authority], Some(&authority.pubkey()))
}

fn send_drain_agent_mandate(
    harness: &mut TestHarness,
    authority: &Keypair,
    authority_spl_usdc_ata: anchor_lang::prelude::Pubkey,
    fixture: &AgentMandateFixture,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let accounts = vela_protocol::accounts::DrainAgentMandate {
        authority: to_anchor_pubkey(authority.pubkey()),
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        agent_mandate: fixture.agent_mandate,
        mandate_wrapped_account: fixture.agent_mandate_wrapped_ata,
        authority_usdc_account: authority_spl_usdc_ata,
        wrapped_usdc_mint: fixture.wrapped_usdc_mint,
        protocol_config: config,
        spl_usdc_mint: fixture.spl_usdc_mint,
        wrapping_vault: fixture.wrapping_vault,
        mint_authority,
        spl_token_program: helpers::to_anchor_pubkey(helpers::spl_token_address()),
        token_2022_program: helpers::to_anchor_pubkey(helpers::token_2022_address()),
    };
    let instruction = Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::DrainAgentMandate {}.data(),
    };
    harness.send_instructions(&[instruction], &[authority], Some(&authority.pubkey()))
}

#[allow(clippy::too_many_arguments)]
fn send_adjust_agent_mandate(
    harness: &mut TestHarness,
    authority: &Keypair,
    fixture: &AgentMandateFixture,
    daily_limit: Option<u64>,
    lifetime_cap: Option<u64>,
    min_pull_amount: Option<u64>,
    min_pull_interval: Option<i64>,
    services: Option<Vec<ServiceLimitInput>>,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let config = harness.derive_config();
    let accounts = vela_protocol::accounts::AdjustAgentMandate {
        authority: to_anchor_pubkey(authority.pubkey()),
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        agent_mandate: fixture.agent_mandate,
        mandate_wrapped_account: fixture.agent_mandate_wrapped_ata,
        wrapped_usdc_mint: fixture.wrapped_usdc_mint,
        protocol_config: config,
        token_2022_program: helpers::to_anchor_pubkey(helpers::token_2022_address()),
    };
    let instruction = Instruction {
        program_id: harness.program_id,
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(helpers::convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::AdjustAgentMandate {
            daily_limit,
            lifetime_cap,
            min_pull_amount,
            min_pull_interval,
            services,
        }
        .data(),
    };
    harness.send_instructions(&[instruction], &[authority], Some(&authority.pubkey()))
}

fn setup_fixture() -> (
    TestHarness,
    Keypair,
    AgentMandateFixture,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
) {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, 8_000_000);
    let service = harness.create_wallet();
    let service_pubkey = to_anchor_pubkey(service.pubkey());
    let service_wrapped_account =
        harness.create_token_2022_ata(&fixture.authority, &service_pubkey, &fixture.wrapped_usdc_mint);
    send_create_agent_mandate(&mut harness, &fixture, service_pubkey, 3_000_000);
    (harness, admin, fixture, service_pubkey, service_wrapped_account)
}

#[test]
fn test_revoke_agent_mandate_blocks_future_pulls() {
    let (mut harness, _admin, fixture, _service, service_wrapped_account) = setup_fixture();
    let payer = harness.create_wallet();

    send_revoke_agent_mandate(
        &mut harness,
        &fixture.authority,
        fixture.authority_spl_usdc_ata,
        &fixture,
    )
    .expect("revoke should succeed");

    let mandate: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert!(matches!(mandate.status, AgentMandateStatus::Revoked));
    let wrapped_balance = harness.fetch_spl_token_account(&fixture.agent_mandate_wrapped_ata);
    assert_eq!(wrapped_balance.amount, 0);
    let authority_balance = harness.fetch_spl_token_account(&fixture.authority_spl_usdc_ata);
    assert_eq!(authority_balance.amount, 8_000_000);

    let error = send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        500_000,
    )
    .expect_err("pull should fail after revoke");
    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom mandate status error, got {:?}",
        error.err,
    );
}

#[test]
fn test_pause_resume_agent_mandate() {
    let (mut harness, _admin, fixture, _service, service_wrapped_account) = setup_fixture();
    let payer = harness.create_wallet();

    send_pause_agent_mandate(&mut harness, &fixture.authority, &fixture)
        .expect("pause should succeed");
    let paused: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert!(matches!(paused.status, AgentMandateStatus::Paused));

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        500_000,
    )
    .expect_err("pull should fail while paused");

    send_resume_agent_mandate(&mut harness, &fixture.authority, &fixture)
        .expect("resume should succeed");
    let resumed: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert!(matches!(resumed.status, AgentMandateStatus::Active));

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        500_000,
    )
    .expect("pull should succeed after resume");
}

#[test]
fn test_drain_agent_mandate_in_all_statuses() {
    // Active status
    {
        let (mut harness, _admin, fixture, _, _) = setup_fixture();
        send_drain_agent_mandate(
            &mut harness,
            &fixture.authority,
            fixture.authority_spl_usdc_ata,
            &fixture,
        )
        .expect("drain should succeed on active mandate");
        let mandate: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
        assert!(matches!(mandate.status, AgentMandateStatus::Active));
        let wrapped = harness.fetch_spl_token_account(&fixture.agent_mandate_wrapped_ata);
        assert_eq!(wrapped.amount, 0);
    }

    // Paused status
    {
        let (mut harness, _admin, fixture, _, _) = setup_fixture();
        send_pause_agent_mandate(&mut harness, &fixture.authority, &fixture)
            .expect("pause should succeed");
        send_drain_agent_mandate(
            &mut harness,
            &fixture.authority,
            fixture.authority_spl_usdc_ata,
            &fixture,
        )
        .expect("drain should succeed on paused mandate");
        let mandate: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
        assert!(matches!(mandate.status, AgentMandateStatus::Paused));
        let wrapped = harness.fetch_spl_token_account(&fixture.agent_mandate_wrapped_ata);
        assert_eq!(wrapped.amount, 0);
    }

    // Revoked status
    {
        let (mut harness, _admin, fixture, _, _) = setup_fixture();
        send_revoke_agent_mandate(
            &mut harness,
            &fixture.authority,
            fixture.authority_spl_usdc_ata,
            &fixture,
        )
        .expect("revoke should succeed");
        send_drain_agent_mandate(
            &mut harness,
            &fixture.authority,
            fixture.authority_spl_usdc_ata,
            &fixture,
        )
        .expect("drain should succeed on revoked mandate");
        let mandate: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
        assert!(matches!(mandate.status, AgentMandateStatus::Revoked));
        let wrapped = harness.fetch_spl_token_account(&fixture.agent_mandate_wrapped_ata);
        assert_eq!(wrapped.amount, 0);
    }
}

#[test]
fn test_lifecycle_authority_only() {
    let (mut harness, _admin, fixture, _service, _service_wrapped_account) = setup_fixture();
    let intruder = harness.create_wallet();
    let intruder_ata = harness.create_spl_token_account(
        &intruder,
        &fixture.spl_usdc_mint,
        &to_anchor_pubkey(intruder.pubkey()),
    );

    send_pause_agent_mandate(&mut harness, &intruder, &fixture)
        .expect_err("non-authority pause should fail");

    send_pause_agent_mandate(&mut harness, &fixture.authority, &fixture)
        .expect("authority pause should succeed");
    send_resume_agent_mandate(&mut harness, &intruder, &fixture)
        .expect_err("non-authority resume should fail");

    send_revoke_agent_mandate(&mut harness, &intruder, intruder_ata, &fixture)
        .expect_err("non-authority revoke should fail");
    send_drain_agent_mandate(&mut harness, &intruder, intruder_ata, &fixture)
        .expect_err("non-authority drain should fail");
}

#[test]
fn test_adjust_agent_mandate_updates_limits() {
    let (mut harness, _admin, fixture, _service, _service_wrapped_account) = setup_fixture();

    send_adjust_agent_mandate(
        &mut harness,
        &fixture.authority,
        &fixture,
        Some(6_500_000),
        Some(25_000_000),
        Some(250_000),
        Some(90),
        None,
    )
    .expect("adjust should succeed");

    let mandate: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert_eq!(mandate.daily_limit, 6_500_000);
    assert_eq!(mandate.lifetime_cap, 25_000_000);
    assert_eq!(mandate.min_pull_amount, 250_000);
    assert_eq!(mandate.min_pull_interval, 90);
}

#[test]
fn test_adjust_agent_mandate_rejects_revoked() {
    let (mut harness, _admin, fixture, _service, _service_wrapped_account) = setup_fixture();
    send_revoke_agent_mandate(
        &mut harness,
        &fixture.authority,
        fixture.authority_spl_usdc_ata,
        &fixture,
    )
    .expect("revoke should succeed");

    let error = send_adjust_agent_mandate(
        &mut harness,
        &fixture.authority,
        &fixture,
        Some(7_000_000),
        None,
        None,
        None,
        None,
    )
    .expect_err("adjust should fail for revoked mandate");
    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom status-transition error, got {:?}",
        error.err,
    );
}

#[test]
fn test_adjust_agent_mandate_rejects_empty_update() {
    let (mut harness, _admin, fixture, _service, _service_wrapped_account) = setup_fixture();
    let error = send_adjust_agent_mandate(
        &mut harness,
        &fixture.authority,
        &fixture,
        None,
        None,
        None,
        None,
        None,
    )
    .expect_err("empty adjust should fail");
    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected no-update custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_adjust_agent_mandate_updates_services() {
    let (mut harness, _admin, fixture, service, service_wrapped_account) = setup_fixture();
    let payer = harness.create_wallet();
    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        500_000,
    )
    .expect("pull should succeed before service adjustment");
    let before: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    let before_service = before
        .services
        .iter()
        .find(|entry| entry.service == service)
        .expect("original service should exist")
        .clone();

    let new_service = to_anchor_pubkey(harness.create_wallet().pubkey());
    send_adjust_agent_mandate(
        &mut harness,
        &fixture.authority,
        &fixture,
        None,
        None,
        None,
        None,
        Some(vec![
            ServiceLimitInput {
                service: new_service,
                daily_limit: 900_000,
            },
            ServiceLimitInput {
                service,
                daily_limit: 3_500_000,
            },
        ]),
    )
    .expect("service update should succeed");

    let mandate: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert_eq!(mandate.services.len(), 2);
    assert_eq!(mandate.services[0].service, new_service);
    assert_eq!(mandate.services[0].daily_spent, 0);
    assert_eq!(mandate.services[1].service, service);
    assert_eq!(mandate.services[1].daily_limit, 3_500_000);
    assert_eq!(mandate.services[1].daily_spent, before_service.daily_spent);
    assert_eq!(mandate.services[1].last_reset, before_service.last_reset);
}

#[test]
fn test_pause_blocks_and_resume_restores_pull() {
    test_pause_resume_agent_mandate();
}

#[test]
fn test_revoke_blocks_future_pulls() {
    test_revoke_agent_mandate_blocks_future_pulls();
}

#[test]
fn test_authority_only_lifecycle_instructions() {
    test_lifecycle_authority_only();
}

#[test]
fn test_drain_reclaims_funds_in_any_status() {
    test_drain_agent_mandate_in_all_statuses();
}
