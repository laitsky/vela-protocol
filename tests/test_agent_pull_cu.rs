#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::{InstructionData, ToAccountMetas};
use helpers::{to_anchor_pubkey, TestHarness};
use solana_instruction::Instruction;
use solana_signer::Signer;

#[test]
fn test_agent_pull_cu_under_300k() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let (fixture, service, service_wrapped_account) =
        harness.agent_mandate_pull_fixture(&admin, 8_000_000);
    let payer = harness.create_wallet();

    let config = harness.derive_config();
    let pull_approval = harness.derive_agent_pull_approval_address(&fixture.agent_mandate);
    let accounts = vela_protocol::accounts::AgentPull {
        payer: to_anchor_pubkey(payer.pubkey()),
        agent: to_anchor_pubkey(fixture.agent.pubkey()),
        authority: to_anchor_pubkey(fixture.authority.pubkey()),
        agent_mandate: fixture.agent_mandate,
        mandate_wrapped_account: fixture.agent_mandate_wrapped_ata,
        service_wrapped_account,
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
        data: vela_protocol::instruction::AgentPull { amount: 700_000 }.data(),
    };

    let meta = harness
        .send_instructions(
            &[instruction],
            &[&payer, &fixture.agent],
            Some(&payer.pubkey()),
        )
        .expect("agent_pull should succeed for CU profiling");
    println!("agent_pull CU: {}", meta.compute_units_consumed);
    assert!(
        meta.compute_units_consumed < 300_000,
        "agent_pull CU too high: {}",
        meta.compute_units_consumed
    );

    let mandate = harness.fetch_agent_mandate(&fixture.agent_mandate);
    let service_entry = mandate
        .services
        .iter()
        .find(|entry| entry.service == service)
        .expect("service entry should exist");
    assert_eq!(service_entry.daily_spent, 700_000);

    let approval_account = harness.svm.get_account(&helpers::to_address(pull_approval));
    assert!(
        approval_account.is_none()
            || approval_account
                .as_ref()
                .map(|account| account.lamports == 0)
                .unwrap_or(false),
        "PullApproval PDA should be absent or zero-lamport after agent_pull",
    );
}
