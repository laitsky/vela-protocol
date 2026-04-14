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

fn serialize_protocol_config(config: &ProtocolConfig) -> Vec<u8> {
    let mut data = ProtocolConfig::DISCRIMINATOR.to_vec();
    config
        .serialize(&mut data)
        .expect("protocol config should serialize");
    data
}

fn serialize_keeper_config(config: &KeeperConfig) -> Vec<u8> {
    let mut data = KeeperConfig::DISCRIMINATOR.to_vec();
    config
        .serialize(&mut data)
        .expect("keeper config should serialize");
    data
}

fn init_keeper_config(
    harness: &mut TestHarness,
    admin: &Keypair,
    protocol_config: Pubkey,
    keeper_authority: Pubkey,
) -> Pubkey {
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
fn test_config_loaders_reject_unsupported_v2_versions() {
    let admin = Pubkey::new_unique();
    let mut protocol_bytes = serialize_protocol_config(&ProtocolConfig::new(
        admin,
        Pubkey::new_unique(),
        ClusterType::Cerberus,
        456,
        protocol_config_address().1,
    ));
    let mut keeper_bytes = serialize_keeper_config(&KeeperConfig::new(
        admin,
        KeeperMode::Centralized,
        b"https://vela.dev".to_vec(),
        Pubkey::new_unique(),
        keeper_config_address().1,
    ));

    let protocol_version_offset = protocol_bytes.len() - 1 - 64;
    protocol_bytes[protocol_version_offset] = 9;
    let keeper_version_offset = keeper_bytes.len() - 1 - 64;
    keeper_bytes[keeper_version_offset] = 9;

    let mut protocol_lamports = 10_000_000;
    let protocol_err = load_protocol_config(&protocol_config_info(
        &mut protocol_bytes,
        &mut protocol_lamports,
    ))
    .err()
    .expect("unsupported protocol config version should be rejected");
    assert_eq!(
        protocol_err,
        anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidAccountData)
    );

    let mut keeper_lamports = 10_000_000;
    let keeper_err =
        load_keeper_config(&keeper_config_info(&mut keeper_bytes, &mut keeper_lamports))
            .err()
            .expect("unsupported keeper config version should be rejected");
    assert_eq!(
        keeper_err,
        anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidAccountData)
    );
}

#[test]
fn test_config_loaders_reject_malformed_legacy_payloads() {
    let admin = Pubkey::new_unique();
    let keeper_authority = Pubkey::new_unique();
    let (protocol_key, protocol_bump) = protocol_config_address();
    let (keeper_key, keeper_bump) = keeper_config_address();
    let mut harness = TestHarness::new();

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
    protocol_bytes.push(1);
    let mut protocol_lamports = 10_000_000;
    let protocol_err = load_protocol_config(&protocol_config_info(
        &mut protocol_bytes,
        &mut protocol_lamports,
    ))
    .err()
    .expect("legacy protocol config with trailing bytes should be rejected");
    assert_eq!(
        protocol_err,
        anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidAccountData)
    );

    let mut keeper_bytes = harness
        .svm
        .get_account(&to_address(keeper_key))
        .expect("keeper config should exist")
        .data;
    keeper_bytes.push(1);
    let mut keeper_lamports = 10_000_000;
    let keeper_err =
        load_keeper_config(&keeper_config_info(&mut keeper_bytes, &mut keeper_lamports))
            .err()
            .expect("legacy keeper config with trailing bytes should be rejected");
    assert_eq!(
        keeper_err,
        anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidAccountData)
    );
}

#[test]
fn test_init_config_and_keeper_config_write_versioned_accounts() {
    let mut harness = TestHarness::new();
    let admin = harness.create_wallet();
    let protocol_config = harness.init_protocol_config(&admin);
    let keeper_config = init_keeper_config(
        &mut harness,
        &admin,
        protocol_config,
        to_anchor_pubkey(admin.pubkey()),
    );

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

#[test]
fn test_runtime_request_callback_wrap_unwrap_readers_reference_compatibility_helpers() {
    let execute_pull = include_str!("../programs/vela-protocol/src/instructions/execute_pull.rs");
    let request_validation =
        include_str!("../programs/vela-protocol/src/instructions/request_validation.rs");
    let request_billing_record =
        include_str!("../programs/vela-protocol/src/instructions/request_billing_record.rs");
    let request_usage_computation =
        include_str!("../programs/vela-protocol/src/instructions/request_usage_computation.rs");
    let validation_callback =
        include_str!("../programs/vela-protocol/src/instructions/validation_callback.rs");
    let billing_callback =
        include_str!("../programs/vela-protocol/src/instructions/billing_callback.rs");
    let usage_computation_callback =
        include_str!("../programs/vela-protocol/src/instructions/usage_computation_callback.rs");
    let wrap = include_str!("../programs/vela-protocol/src/instructions/wrap.rs");
    let unwrap = include_str!("../programs/vela-protocol/src/instructions/unwrap.rs");

    // execute_pull uses both load_keeper_config and load_protocol_config
    assert!(
        execute_pull.contains("load_keeper_config"),
        "execute_pull must use load_keeper_config"
    );
    assert!(
        execute_pull.contains("load_protocol_config"),
        "execute_pull must use load_protocol_config"
    );

    // request flows use load_protocol_config
    assert!(
        request_validation.contains("load_protocol_config"),
        "request_validation must use load_protocol_config"
    );
    assert!(
        request_billing_record.contains("load_protocol_config"),
        "request_billing_record must use load_protocol_config"
    );
    assert!(
        request_usage_computation.contains("load_protocol_config"),
        "request_usage_computation must use load_protocol_config"
    );

    // callback flows use load_protocol_config
    assert!(
        validation_callback.contains("load_protocol_config"),
        "validation_callback must use load_protocol_config"
    );
    assert!(
        billing_callback.contains("load_protocol_config"),
        "billing_callback must use load_protocol_config"
    );
    assert!(
        usage_computation_callback.contains("load_protocol_config"),
        "usage_computation_callback must use load_protocol_config"
    );

    // wrap/unwrap use load_protocol_config
    assert!(
        wrap.contains("load_protocol_config"),
        "wrap must use load_protocol_config"
    );
    assert!(
        unwrap.contains("load_protocol_config"),
        "unwrap must use load_protocol_config"
    );
}
