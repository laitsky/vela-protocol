#![allow(clippy::result_large_err, clippy::too_many_arguments)]

#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::{InstructionData, ToAccountMetas};
use helpers::{to_anchor_pubkey, AgentMandateFixture, TestHarness};
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{instructions::ServiceLimitInput, state::AgentMandate};

fn send_create_agent_mandate(
    harness: &mut TestHarness,
    fixture: &AgentMandateFixture,
    daily_limit: u64,
    lifetime_cap: u64,
    min_pull_amount: u64,
    min_pull_interval: i64,
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
            daily_limit,
            lifetime_cap,
            min_pull_amount,
            min_pull_interval,
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
        token_config: fixture.token_config,
        protocol_config: config,
        wrapping_vault: fixture.wrapping_vault,
        hook_program: anchor_lang::prelude::Pubkey::new_from_array(
            vela_transfer_hook::ID.to_bytes(),
        ),
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

fn setup_single_service_fixture(
    mandate_daily_limit: u64,
    service_daily_limit: u64,
    lifetime_cap: u64,
    min_pull_amount: u64,
    min_pull_interval: i64,
    funded_amount: u64,
) -> (
    TestHarness,
    AgentMandateFixture,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
) {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, 10_000_000);
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
        mandate_daily_limit,
        lifetime_cap,
        min_pull_amount,
        min_pull_interval,
        vec![ServiceLimitInput {
            service: service_pubkey,
            daily_limit: service_daily_limit,
        }],
        funded_amount,
    );

    (harness, fixture, service_pubkey, service_wrapped_account)
}

fn setup_two_service_fixture() -> (
    TestHarness,
    AgentMandateFixture,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
    anchor_lang::prelude::Pubkey,
) {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, 12_000_000);
    let service_one = harness.create_wallet();
    let service_two = harness.create_wallet();
    let service_one_pubkey = to_anchor_pubkey(service_one.pubkey());
    let service_two_pubkey = to_anchor_pubkey(service_two.pubkey());
    let service_one_wrapped = harness.create_token_2022_ata(
        &fixture.authority,
        &service_one_pubkey,
        &fixture.wrapped_usdc_mint,
    );
    let service_two_wrapped = harness.create_token_2022_ata(
        &fixture.authority,
        &service_two_pubkey,
        &fixture.wrapped_usdc_mint,
    );

    send_create_agent_mandate(
        &mut harness,
        &fixture,
        9_000_000,
        20_000_000,
        100_000,
        0,
        vec![
            ServiceLimitInput {
                service: service_one_pubkey,
                daily_limit: 2_000_000,
            },
            ServiceLimitInput {
                service: service_two_pubkey,
                daily_limit: 2_000_000,
            },
        ],
        7_000_000,
    );

    (
        harness,
        fixture,
        service_one_pubkey,
        service_one_wrapped,
        service_two_pubkey,
        service_two_wrapped,
    )
}

#[test]
fn test_agent_pull_within_daily_limit_and_exceed_fail() {
    let (mut harness, fixture, _service, service_wrapped_account) =
        setup_single_service_fixture(2_000_000, 2_000_000, 20_000_000, 100_000, 0, 3_000_000);
    let payer = harness.create_wallet();

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        2_000_000,
    )
    .expect("pull at exact daily limit should succeed");

    let error = send_agent_pull(&mut harness, &fixture, &payer, &service_wrapped_account, 1)
        .expect_err("pull above daily limit should fail");
    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom daily-limit error, got {:?}",
        error.err,
    );
}

#[test]
fn test_agent_pull_within_lifetime_cap_and_exceed_fail() {
    let (mut harness, fixture, _service, service_wrapped_account) =
        setup_single_service_fixture(10_000_000, 10_000_000, 2_100_000, 100_000, 0, 3_000_000);
    let payer = harness.create_wallet();

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        2_000_000,
    )
    .expect("pull under lifetime cap should succeed");

    let error = send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        200_000,
    )
    .expect_err("pull above lifetime cap should fail");
    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom lifetime-cap error, got {:?}",
        error.err,
    );
}

#[test]
fn test_agent_pull_per_service_limits_are_independent() {
    let (mut harness, fixture, service_one, service_one_wrapped, service_two, service_two_wrapped) =
        setup_two_service_fixture();
    let payer = harness.create_wallet();

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_one_wrapped,
        2_000_000,
    )
    .expect("service one should pull up to its own limit");

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_two_wrapped,
        1_500_000,
    )
    .expect("service two should pull independently");

    let mandate: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    let service_one_state = mandate
        .services
        .iter()
        .find(|entry| entry.service == service_one)
        .expect("service one should exist");
    let service_two_state = mandate
        .services
        .iter()
        .find(|entry| entry.service == service_two)
        .expect("service two should exist");
    assert_eq!(service_one_state.daily_spent, 2_000_000);
    assert_eq!(service_two_state.daily_spent, 1_500_000);
}

#[test]
fn test_agent_pull_resets_daily_spent_after_24h() {
    let (mut harness, fixture, service, service_wrapped_account) =
        setup_single_service_fixture(2_000_000, 2_000_000, 20_000_000, 100_000, 0, 4_000_000);
    let payer = harness.create_wallet();

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        2_000_000,
    )
    .expect("first pull should consume initial window");
    let before_warp: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    let before_service = before_warp
        .services
        .iter()
        .find(|entry| entry.service == service)
        .expect("service should exist");
    assert_eq!(before_warp.daily_spent, 2_000_000);
    assert_eq!(before_service.daily_spent, 2_000_000);

    harness.set_clock_timestamp(before_warp.daily_last_reset + 86_400);
    let pre_reset_state: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert_eq!(pre_reset_state.daily_spent, 2_000_000);
    let pre_reset_service = pre_reset_state
        .services
        .iter()
        .find(|entry| entry.service == service)
        .expect("service should exist");
    assert_eq!(pre_reset_service.daily_spent, 2_000_000);

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        1_000_000,
    )
    .expect("pull exactly at 24h boundary should lazily reset then succeed");

    let after_reset: AgentMandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    let after_service = after_reset
        .services
        .iter()
        .find(|entry| entry.service == service)
        .expect("service should exist");
    assert_eq!(after_reset.daily_spent, 1_000_000);
    assert_eq!(after_service.daily_spent, 1_000_000);
}

#[test]
fn test_agent_pull_rejects_unauthorized_service() {
    let (mut harness, fixture, _service, _service_wrapped_account) =
        setup_single_service_fixture(2_000_000, 2_000_000, 20_000_000, 100_000, 0, 3_000_000);
    let payer = harness.create_wallet();
    let unauthorized_owner = harness.create_wallet();
    let unauthorized_owner_pubkey = to_anchor_pubkey(unauthorized_owner.pubkey());
    let unauthorized_wrapped_account = harness.create_token_2022_ata(
        &fixture.authority,
        &unauthorized_owner_pubkey,
        &fixture.wrapped_usdc_mint,
    );

    let error = send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &unauthorized_wrapped_account,
        500_000,
    )
    .expect_err("pull should fail for unauthorized service");
    assert!(
        format!("{:?}", error.err).contains("Custom("),
        "expected custom unauthorized-service error, got {:?}",
        error.err,
    );
}

#[test]
fn test_agent_pull_enforces_min_amount_and_cooldown() {
    let (mut harness, fixture, _service, service_wrapped_account) = setup_single_service_fixture(
        5_000_000, 5_000_000, 20_000_000, 500_000, 1_000_000, 4_000_000,
    );
    let payer = harness.create_wallet();
    harness.set_clock_timestamp(1_000_000);

    let too_small = send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        499_999,
    )
    .expect_err("pull below min_pull_amount should fail");
    assert!(
        format!("{:?}", too_small.err).contains("Custom("),
        "expected custom min-amount error, got {:?}",
        too_small.err,
    );

    send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        600_000,
    )
    .expect("pull above min amount should succeed");

    let cooldown = send_agent_pull(
        &mut harness,
        &fixture,
        &payer,
        &service_wrapped_account,
        600_000,
    )
    .expect_err("second pull within cooldown should fail");
    assert!(
        format!("{:?}", cooldown.err).contains("Custom("),
        "expected custom cooldown error, got {:?}",
        cooldown.err,
    );
}
