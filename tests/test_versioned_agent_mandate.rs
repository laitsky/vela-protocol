#[path = "helpers/mod.rs"]
mod helpers;

use std::fs;
use std::path::PathBuf;

use anchor_lang::{
    prelude::{borsh, Pubkey},
    AnchorDeserialize, AnchorSerialize, Discriminator, InstructionData, ToAccountMetas,
};
use helpers::TestHarness;
use solana_instruction::Instruction;
use solana_signer::Signer;
use vela_protocol::{
    instructions::{agent_mandate_account::load_agent_mandate, ServiceLimitInput},
    state::{AgentMandate, AgentMandateStatus, ServiceLimit},
};

#[derive(AnchorSerialize, AnchorDeserialize)]
struct LegacyAgentMandate {
    authority: Pubkey,
    agent: Pubkey,
    daily_limit: u64,
    daily_spent: u64,
    daily_last_reset: i64,
    lifetime_cap: u64,
    total_spent: u64,
    min_pull_amount: u64,
    min_pull_interval: i64,
    last_pull_at: i64,
    status: AgentMandateStatus,
    services: Vec<ServiceLimit>,
    bump: u8,
}

fn legacy_agent_mandate_info<'a>(
    key: &'a Pubkey,
    bytes: &'a mut Vec<u8>,
    lamports: &'a mut u64,
) -> anchor_lang::prelude::AccountInfo<'a> {
    let owner = Box::leak(Box::new(vela_protocol::ID));
    anchor_lang::prelude::AccountInfo::new(
        key,
        false,
        true,
        lamports,
        bytes.as_mut_slice(),
        owner,
        false,
        0,
    )
}

#[test]
fn test_agent_mandate_loader_accepts_legacy_bytes() {
    let authority = Pubkey::new_unique();
    let agent = Pubkey::new_unique();
    let (mandate_key, bump) = Pubkey::find_program_address(
        &[b"agent-mandate", authority.as_ref(), agent.as_ref()],
        &vela_protocol::ID,
    );
    let legacy = LegacyAgentMandate {
        authority,
        agent,
        daily_limit: 5_000_000,
        daily_spent: 250_000,
        daily_last_reset: 123,
        lifetime_cap: 20_000_000,
        total_spent: 750_000,
        min_pull_amount: 100_000,
        min_pull_interval: 60,
        last_pull_at: 99,
        status: AgentMandateStatus::Paused,
        services: vec![ServiceLimit {
            service: Pubkey::new_unique(),
            daily_limit: 2_500_000,
            daily_spent: 125_000,
            last_reset: 77,
        }],
        bump,
    };

    let mut bytes = AgentMandate::DISCRIMINATOR.to_vec();
    legacy
        .serialize(&mut bytes)
        .expect("legacy agent mandate should serialize");
    let mut lamports = 10_000_000;

    let loaded = load_agent_mandate(
        &legacy_agent_mandate_info(&mandate_key, &mut bytes, &mut lamports),
        &authority,
        &agent,
    )
    .expect("legacy mandate should load")
    .into_current();

    assert_eq!(loaded.authority, authority);
    assert_eq!(loaded.agent, agent);
    assert_eq!(
        loaded.version,
        vela_protocol::state::CURRENT_ACCOUNT_VERSION
    );
    assert_eq!(
        loaded._reserved,
        [0u8; vela_protocol::state::ACCOUNT_RESERVED_BYTES]
    );
    assert_eq!(loaded.status, AgentMandateStatus::Paused);
    assert_eq!(loaded.services.len(), 1);
}

#[test]
fn test_create_agent_mandate_writes_versioned_accounts() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, 10_000_000);
    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();

    let accounts = vela_protocol::accounts::CreateAgentMandate {
        authority: helpers::to_anchor_pubkey(fixture.authority.pubkey()),
        agent: helpers::to_anchor_pubkey(fixture.agent.pubkey()),
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
                service: Pubkey::new_unique(),
                daily_limit: 4_000_000,
            }],
            funded_amount: 1_000_000,
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

    let mandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert_eq!(
        mandate.version,
        vela_protocol::state::CURRENT_ACCOUNT_VERSION
    );
    assert_eq!(
        mandate._reserved,
        [0u8; vela_protocol::state::ACCOUNT_RESERVED_BYTES]
    );
}

#[test]
fn test_agent_mandate_consumers_use_compatibility_loader() {
    let instruction_path = |name: &str| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/instructions")
            .join(name)
    };

    let create = fs::read_to_string(instruction_path("create_agent_mandate.rs"))
        .expect("create_agent_mandate.rs should exist");
    let adjust = fs::read_to_string(instruction_path("adjust_agent_mandate.rs"))
        .expect("adjust_agent_mandate.rs should exist");
    let pause = fs::read_to_string(instruction_path("pause_agent_mandate.rs"))
        .expect("pause_agent_mandate.rs should exist");
    let resume = fs::read_to_string(instruction_path("resume_agent_mandate.rs"))
        .expect("resume_agent_mandate.rs should exist");
    let revoke = fs::read_to_string(instruction_path("revoke_agent_mandate.rs"))
        .expect("revoke_agent_mandate.rs should exist");
    let drain = fs::read_to_string(instruction_path("drain_agent_mandate.rs"))
        .expect("drain_agent_mandate.rs should exist");
    let agent_pull =
        fs::read_to_string(instruction_path("agent_pull.rs")).expect("agent_pull.rs should exist");

    assert!(create.contains("load_agent_mandate"));
    assert!(adjust.contains("load_agent_mandate"));
    assert!(pause.contains("load_agent_mandate"));
    assert!(resume.contains("load_agent_mandate"));
    assert!(revoke.contains("load_agent_mandate"));
    assert!(drain.contains("load_agent_mandate"));
    assert!(agent_pull.contains("load_agent_mandate"));
}

// Helper to send adjust_agent_mandate
fn send_adjust_agent_mandate(
    harness: &mut TestHarness,
    authority: &solana_keypair::Keypair,
    fixture: &helpers::AgentMandateFixture,
    daily_limit: Option<u64>,
    lifetime_cap: Option<u64>,
    min_pull_amount: Option<u64>,
    min_pull_interval: Option<i64>,
    services: Option<Vec<ServiceLimitInput>>,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let config = harness.derive_config();
    let accounts = vela_protocol::accounts::AdjustAgentMandate {
        authority: helpers::to_anchor_pubkey(authority.pubkey()),
        agent: helpers::to_anchor_pubkey(fixture.agent.pubkey()),
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

fn send_pause_agent_mandate(
    harness: &mut TestHarness,
    authority: &solana_keypair::Keypair,
    fixture: &helpers::AgentMandateFixture,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let accounts = vela_protocol::accounts::PauseAgentMandate {
        authority: helpers::to_anchor_pubkey(authority.pubkey()),
        agent: helpers::to_anchor_pubkey(fixture.agent.pubkey()),
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
    authority: &solana_keypair::Keypair,
    fixture: &helpers::AgentMandateFixture,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let accounts = vela_protocol::accounts::ResumeAgentMandate {
        authority: helpers::to_anchor_pubkey(authority.pubkey()),
        agent: helpers::to_anchor_pubkey(fixture.agent.pubkey()),
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

fn send_drain_agent_mandate(
    harness: &mut TestHarness,
    authority: &solana_keypair::Keypair,
    fixture: &helpers::AgentMandateFixture,
) -> Result<litesvm::types::TransactionMetadata, litesvm::types::FailedTransactionMetadata> {
    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let accounts = vela_protocol::accounts::DrainAgentMandate {
        authority: helpers::to_anchor_pubkey(authority.pubkey()),
        agent: helpers::to_anchor_pubkey(fixture.agent.pubkey()),
        agent_mandate: fixture.agent_mandate,
        mandate_wrapped_account: fixture.agent_mandate_wrapped_ata,
        authority_usdc_account: fixture.authority_spl_usdc_ata,
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

fn create_versioned_agent_mandate(
    harness: &mut TestHarness,
    fixture: &helpers::AgentMandateFixture,
    service: Pubkey,
    funded_amount: u64,
) {
    let config = harness.derive_config();
    let (mint_authority, _) = harness.derive_mint_authority();
    let accounts = vela_protocol::accounts::CreateAgentMandate {
        authority: helpers::to_anchor_pubkey(fixture.authority.pubkey()),
        agent: helpers::to_anchor_pubkey(fixture.agent.pubkey()),
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

fn assert_versioned(mandate: &AgentMandate, label: &str) {
    assert_eq!(
        mandate.version,
        vela_protocol::state::CURRENT_ACCOUNT_VERSION,
        "{label}: version must be CURRENT_ACCOUNT_VERSION"
    );
    assert_eq!(
        mandate._reserved,
        [0u8; vela_protocol::state::ACCOUNT_RESERVED_BYTES],
        "{label}: _reserved must be zeroed"
    );
}

#[test]
fn test_adjust_preserves_versioned_layout() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, 10_000_000);
    let service = Pubkey::new_unique();

    create_versioned_agent_mandate(&mut harness, &fixture, service, 3_000_000);

    // Adjust limits
    send_adjust_agent_mandate(
        &mut harness,
        &fixture.authority,
        &fixture,
        Some(7_000_000),
        Some(25_000_000),
        Some(200_000),
        Some(120),
        None,
    )
    .expect("adjust should succeed");

    let mandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert_versioned(&mandate, "after adjust");
    assert_eq!(mandate.daily_limit, 7_000_000);
    assert_eq!(mandate.lifetime_cap, 25_000_000);
    assert_eq!(mandate.min_pull_amount, 200_000);
    assert_eq!(mandate.min_pull_interval, 120);
}

#[test]
fn test_pause_resume_preserve_versioned_layout() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, 10_000_000);
    let service = Pubkey::new_unique();

    create_versioned_agent_mandate(&mut harness, &fixture, service, 3_000_000);

    // Pause
    send_pause_agent_mandate(&mut harness, &fixture.authority, &fixture)
        .expect("pause should succeed");
    let paused = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert_versioned(&paused, "after pause");
    assert!(matches!(paused.status, AgentMandateStatus::Paused));

    // Resume
    send_resume_agent_mandate(&mut harness, &fixture.authority, &fixture)
        .expect("resume should succeed");
    let resumed = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert_versioned(&resumed, "after resume");
    assert!(matches!(resumed.status, AgentMandateStatus::Active));
}

#[test]
fn test_drain_preserves_versioned_layout() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let fixture = harness.setup_agent_mandate_fixture(&admin, 10_000_000);
    let service = Pubkey::new_unique();

    create_versioned_agent_mandate(&mut harness, &fixture, service, 3_000_000);

    // Drain
    send_drain_agent_mandate(&mut harness, &fixture.authority, &fixture)
        .expect("drain should succeed");
    let drained = harness.fetch_agent_mandate(&fixture.agent_mandate);
    assert_versioned(&drained, "after drain");
    assert!(matches!(drained.status, AgentMandateStatus::Active));
    let wrapped = harness.fetch_spl_token_account(&fixture.agent_mandate_wrapped_ata);
    assert_eq!(wrapped.amount, 0);
}

#[test]
fn test_legacy_agent_mandate_roundtrip_preserves_fields() {
    let authority = Pubkey::new_unique();
    let agent = Pubkey::new_unique();
    let service = Pubkey::new_unique();
    let (mandate_key, bump) = Pubkey::find_program_address(
        &[b"agent-mandate", authority.as_ref(), agent.as_ref()],
        &vela_protocol::ID,
    );

    let legacy = LegacyAgentMandate {
        authority,
        agent,
        daily_limit: 8_000_000,
        daily_spent: 500_000,
        daily_last_reset: 42,
        lifetime_cap: 50_000_000,
        total_spent: 1_200_000,
        min_pull_amount: 50_000,
        min_pull_interval: 30,
        last_pull_at: 77,
        status: AgentMandateStatus::Active,
        services: vec![
            ServiceLimit {
                service,
                daily_limit: 3_000_000,
                daily_spent: 200_000,
                last_reset: 55,
            },
            ServiceLimit {
                service: Pubkey::new_unique(),
                daily_limit: 5_000_000,
                daily_spent: 300_000,
                last_reset: 66,
            },
        ],
        bump,
    };

    let mut bytes = AgentMandate::DISCRIMINATOR.to_vec();
    legacy
        .serialize(&mut bytes)
        .expect("legacy should serialize");
    let mut lamports = 10_000_000;

    let loaded = load_agent_mandate(
        &legacy_agent_mandate_info(&mandate_key, &mut bytes, &mut lamports),
        &authority,
        &agent,
    )
    .expect("legacy should load");

    // Verify it detected as legacy
    assert!(loaded.is_legacy());

    // Verify every field is preserved in the converted form
    let current = loaded.into_current();
    assert_eq!(current.authority, authority);
    assert_eq!(current.agent, agent);
    assert_eq!(current.daily_limit, 8_000_000);
    assert_eq!(current.daily_spent, 500_000);
    assert_eq!(current.daily_last_reset, 42);
    assert_eq!(current.lifetime_cap, 50_000_000);
    assert_eq!(current.total_spent, 1_200_000);
    assert_eq!(current.min_pull_amount, 50_000);
    assert_eq!(current.min_pull_interval, 30);
    assert_eq!(current.last_pull_at, 77);
    assert!(matches!(current.status, AgentMandateStatus::Active));
    assert_eq!(current.services.len(), 2);
    assert_eq!(current.services[0].service, service);
    assert_eq!(current.services[0].daily_limit, 3_000_000);
    assert_eq!(current.services[0].daily_spent, 200_000);
    assert_eq!(current.services[0].last_reset, 55);
    assert_eq!(current.services[1].daily_limit, 5_000_000);
    assert_eq!(current.services[1].daily_spent, 300_000);
    assert_eq!(current.bump, bump);
    assert_eq!(
        current.version,
        vela_protocol::state::CURRENT_ACCOUNT_VERSION
    );
    assert_eq!(
        current._reserved,
        [0u8; vela_protocol::state::ACCOUNT_RESERVED_BYTES]
    );
}
