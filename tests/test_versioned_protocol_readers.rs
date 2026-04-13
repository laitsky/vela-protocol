#[path = "helpers/mod.rs"]
mod helpers;

use anchor_lang::{
    prelude::{borsh, AccountInfo, Pubkey},
    AnchorDeserialize, AnchorSerialize, Discriminator, InstructionData, ToAccountMetas,
};
use helpers::{convert_account_meta, to_address, to_anchor_pubkey, TestHarness};
use solana_account::Account;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_signer::Signer;
use vela_protocol::{
    instructions::{
        keeper_config_account::load_keeper_config,
        protocol_config_account::load_protocol_config,
    },
    state::{ClusterType, KeeperConfig, KeeperMode, ProtocolConfig},
};

#[derive(AnchorSerialize, AnchorDeserialize)]
struct LegacyProtocolConfig {
    admin: Pubkey,
    cluster_pubkey: Pubkey,
    cluster_type: ClusterType,
    cluster_offset: u64,
    wrapped_usdc_mint: Pubkey,
    wrapping_vault: Pubkey,
    paused: bool,
    paused_at: i64,
    bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
struct LegacyKeeperConfig {
    admin: Pubkey,
    mode: KeeperMode,
    keeper_endpoint: [u8; 128],
    endpoint_len: u8,
    keeper_authority: Pubkey,
    bump: u8,
}

fn protocol_config_address() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ProtocolConfig::SEED_PREFIX], &vela_protocol::ID)
}

fn keeper_config_address() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[KeeperConfig::SEED_PREFIX], &vela_protocol::ID)
}

fn write_legacy_protocol_config(harness: &mut TestHarness, config: Pubkey, admin: Pubkey, bump: u8) {
    let legacy = LegacyProtocolConfig {
        admin,
        cluster_pubkey: Pubkey::new_unique(),
        cluster_type: ClusterType::Cerberus,
        cluster_offset: 456,
        wrapped_usdc_mint: Pubkey::new_unique(),
        wrapping_vault: Pubkey::new_unique(),
        paused: true,
        paused_at: 123,
        bump,
    };
    let mut data = ProtocolConfig::DISCRIMINATOR.to_vec();
    legacy
        .serialize(&mut data)
        .expect("legacy protocol config should serialize");

    harness
        .svm
        .set_account(
            to_address(config),
            Account {
                lamports: 10_000_000,
                data,
                owner: to_address(vela_protocol::ID),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("legacy protocol config should write");
}

fn write_legacy_keeper_config(
    harness: &mut TestHarness,
    config: Pubkey,
    admin: Pubkey,
    keeper_authority: Pubkey,
    bump: u8,
) {
    let mut endpoint = [0u8; 128];
    endpoint[..16].copy_from_slice(b"https://vela.dev");
    let legacy = LegacyKeeperConfig {
        admin,
        mode: KeeperMode::Centralized,
        keeper_endpoint: endpoint,
        endpoint_len: 16,
        keeper_authority,
        bump,
    };
    let mut data = KeeperConfig::DISCRIMINATOR.to_vec();
    legacy
        .serialize(&mut data)
        .expect("legacy keeper config should serialize");

    harness
        .svm
        .set_account(
            to_address(config),
            Account {
                lamports: 10_000_000,
                data,
                owner: to_address(vela_protocol::ID),
                executable: false,
                rent_epoch: 0,
            },
        )
        .expect("legacy keeper config should write");
}

fn protocol_config_info<'a>(bytes: &'a mut Vec<u8>, lamports: &'a mut u64) -> AccountInfo<'a> {
    let key = Box::leak(Box::new(protocol_config_address().0));
    let owner = Box::leak(Box::new(vela_protocol::ID));
    AccountInfo::new(
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

fn keeper_config_info<'a>(bytes: &'a mut Vec<u8>, lamports: &'a mut u64) -> AccountInfo<'a> {
    let key = Box::leak(Box::new(keeper_config_address().0));
    let owner = Box::leak(Box::new(vela_protocol::ID));
    AccountInfo::new(
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

fn init_keeper_config(harness: &mut TestHarness, admin: &Keypair, keeper_authority: Pubkey) -> Pubkey {
    let protocol_config = harness.init_protocol_config(admin);
    let (keeper_config, _) = keeper_config_address();
    let accounts = vela_protocol::accounts::InitKeeperConfig {
        admin: to_anchor_pubkey(admin.pubkey()),
        protocol_config,
        keeper_config,
        system_program: anchor_lang::system_program::ID,
    };
    let instruction = Instruction {
        program_id: to_address(vela_protocol::ID),
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(convert_account_meta)
            .collect(),
        data: vela_protocol::instruction::InitKeeperConfig {
            mode: KeeperMode::Centralized,
            keeper_endpoint: b"https://vela.dev".to_vec(),
            keeper_authority,
        }
        .data(),
    };
    harness
        .send_instructions(&[instruction], &[admin], Some(&admin.pubkey()))
        .expect("init_keeper_config should succeed");
    keeper_config
}

#[test]
fn test_protocol_and_keeper_loaders_accept_legacy_singletons() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant_pubkey();
    let keeper_authority = Pubkey::new_unique();
    let (protocol_key, protocol_bump) = protocol_config_address();
    let (keeper_key, keeper_bump) = keeper_config_address();

    write_legacy_protocol_config(&mut harness, protocol_key, admin, protocol_bump);
    write_legacy_keeper_config(
        &mut harness,
        keeper_key,
        admin,
        keeper_authority,
        keeper_bump,
    );

    let mut protocol_bytes = harness
        .svm
        .get_account(&to_address(protocol_key))
        .expect("protocol config should exist")
        .data;
    let mut protocol_lamports = 10_000_000;
    let protocol = load_protocol_config(&protocol_config_info(
        &mut protocol_bytes,
        &mut protocol_lamports,
    ))
    .expect("legacy protocol config should load")
    .into_current();

    let mut keeper_bytes = harness
        .svm
        .get_account(&to_address(keeper_key))
        .expect("keeper config should exist")
        .data;
    let mut keeper_lamports = 10_000_000;
    let keeper = load_keeper_config(&keeper_config_info(&mut keeper_bytes, &mut keeper_lamports))
        .expect("legacy keeper config should load")
        .into_current();

    assert_eq!(protocol.admin, admin);
    assert_eq!(protocol.cluster_offset, 456);
    assert_eq!(protocol.version, 1);
    assert_eq!(protocol._reserved, [0u8; 64]);
    assert_eq!(keeper.admin, admin);
    assert_eq!(keeper.keeper_authority, keeper_authority);
    assert_eq!(keeper.version, 1);
    assert_eq!(keeper._reserved, [0u8; 64]);
}

#[test]
fn test_init_config_and_keeper_config_write_versioned_accounts() {
    let mut harness = TestHarness::new();
    let admin = harness.create_wallet();
    let protocol_config = harness.init_protocol_config(&admin);
    let keeper_config = init_keeper_config(&mut harness, &admin, to_anchor_pubkey(admin.pubkey()));

    let protocol: ProtocolConfig = harness.fetch_anchor_account(&protocol_config);
    let keeper: KeeperConfig = harness.fetch_anchor_account(&keeper_config);

    assert_eq!(protocol.version, 1);
    assert_eq!(protocol._reserved, [0u8; 64]);
    assert_eq!(keeper.version, 1);
    assert_eq!(keeper._reserved, [0u8; 64]);
}

#[test]
fn test_initializer_admin_consumers_reference_compatibility_helpers() {
    let init_config = include_str!("../programs/vela-protocol/src/instructions/init_config.rs");
    let init_keeper_config =
        include_str!("../programs/vela-protocol/src/instructions/init_keeper_config.rs");
    let update_keeper_config =
        include_str!("../programs/vela-protocol/src/instructions/update_keeper_config.rs");
    let init_wrapped_mint =
        include_str!("../programs/vela-protocol/src/instructions/init_wrapped_mint.rs");
    let admin_cancel = include_str!("../programs/vela-protocol/src/instructions/admin_cancel.rs");
    let pause_protocol =
        include_str!("../programs/vela-protocol/src/instructions/pause_protocol.rs");
    let unpause_protocol =
        include_str!("../programs/vela-protocol/src/instructions/unpause_protocol.rs");

    assert!(init_config.contains("load_protocol_config"));
    assert!(init_keeper_config.contains("load_protocol_config"));
    assert!(update_keeper_config.contains("load_protocol_config"));
    assert!(update_keeper_config.contains("load_keeper_config"));
    assert!(init_wrapped_mint.contains("load_protocol_config"));
    assert!(admin_cancel.contains("load_protocol_config"));
    assert!(pause_protocol.contains("load_protocol_config"));
    assert!(unpause_protocol.contains("load_protocol_config"));
}
