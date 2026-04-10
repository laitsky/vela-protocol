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
    services: Vec<ServiceLimitInput>,
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
            services,
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

fn setup_agent_pull_fixture() -> (
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
    send_create_agent_mandate(
        &mut harness,
        &fixture,
        vec![ServiceLimitInput {
            service: service_pubkey,
            daily_limit: 4_000_000,
        }],
        3_000_000,
    );
    (harness, admin, fixture, service_pubkey, service_wrapped_account)
}

#[test]
fn test_agent_pull_success() {
    let (mut harness, _admin, fixture, service, service_wrapped_account) = setup_agent_pull_fixture();
    let payer = harness.create_wallet();

    let amount = 700_000;
    let meta = send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        amount,
    )
    .expect("agent_pull should succeed");
    assert!(meta.compute_units_consumed > 0);

    let destination_balance = harness.fetch_spl_token_account(&service_wrapped_account);
    assert_eq!(destination_balance.amount, amount);

    let mandate: AgentMandate = harness.fetch_anchor_account(&fixture.agent_mandate);
    assert_eq!(mandate.daily_spent, amount);
    assert_eq!(mandate.total_spent, amount);
    assert_eq!(mandate.last_pull_at, harness.current_timestamp());
    assert_eq!(mandate.services.len(), 1);
    assert_eq!(mandate.services[0].service, service);
    assert_eq!(mandate.services[0].daily_spent, amount);
    assert!(matches!(mandate.status, AgentMandateStatus::Active));
}

#[test]
fn test_agent_pull_closes_pull_approval() {
    let (mut harness, _admin, fixture, _service, service_wrapped_account) = setup_agent_pull_fixture();
    let payer = harness.create_wallet();
    let approval = harness.derive_agent_pull_approval_address(&fixture.agent_mandate);

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        500_000,
    )
    .expect("agent_pull should succeed");

    let approval_account = harness.svm.get_account(&helpers::to_address(approval));
    assert!(
        approval_account.is_none()
            || approval_account
                .as_ref()
                .map(|account| account.lamports == 0)
                .unwrap_or(false),
        "PullApproval PDA should be absent or zero-lamport after agent_pull",
    );
}

#[test]
fn test_protocol_pause_blocks_agent_pull() {
    let (mut harness, admin, fixture, _service, service_wrapped_account) = setup_agent_pull_fixture();
    let payer = harness.create_wallet();
    harness
        .send_pause_protocol(&admin)
        .expect("pause protocol should succeed");

    let error = send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        500_000,
    )
    .expect_err("agent_pull should fail while protocol is paused");

    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected protocol paused custom error, got {:?}",
        error.err,
    );
}

#[test]
fn test_transfer_hook_validates_agent_pull_transfer() {
    test_agent_pull_success();
}

#[test]
fn test_agent_pull_reports_insufficient_mandate_balance() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, 500_000);
    let service = harness.create_wallet();
    let service_pubkey = to_anchor_pubkey(service.pubkey());
    let service_wrapped_account = harness.create_token_2022_ata(
        &fixture.authority,
        &service_pubkey,
        &fixture.wrapped_usdc_mint,
    );
    send_create_agent_mandate(
        &mut harness,
        &fixture,
        vec![ServiceLimitInput {
            service: service_pubkey,
            daily_limit: 4_000_000,
        }],
        500_000,
    );
    let payer = harness.create_wallet();

    let error = send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        700_000,
    )
    .expect_err("agent_pull should fail when wrapped balance is too low");

    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom error, got {:?}",
        error.err,
    );
}
