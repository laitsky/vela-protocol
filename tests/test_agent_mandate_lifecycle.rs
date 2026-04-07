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
