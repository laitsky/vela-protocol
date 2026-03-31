//! Helper functions for Arcium-related test setup using LiteSVM.
//!
//! Since actual Arcium MXE initialization requires a running Arcium cluster (Docker),
//! these helpers provide mock account structures for instruction account validation testing.

use anchor_lang::{prelude::Pubkey, AccountSerialize};
use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use vela_protocol::state::{ClusterType, ProtocolConfig};

/// Derives the ProtocolConfig PDA address from seeds [b"config"] and program_id.
pub fn derive_config_pda(program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"config"], program_id).0
}

/// Creates a mock ProtocolConfig account in LiteSVM at the expected PDA.
///
/// This simulates the state after `init_config` has been executed, allowing
/// tests to verify subsequent instructions that read from the ProtocolConfig.
pub fn create_mock_protocol_config(
    svm: &mut LiteSVM,
    admin_pubkey: &Pubkey,
    cluster_pubkey: &Pubkey,
    cluster_type: ClusterType,
    cluster_offset: u64,
    program_id: &Pubkey,
) -> Pubkey {
    let config_pda = derive_config_pda(program_id);

    let config = ProtocolConfig {
        admin: *admin_pubkey,
        cluster_pubkey: *cluster_pubkey,
        cluster_type,
        cluster_offset,
        bump: 255, // Mock bump; the actual bump would be derived
    };

    let mut data = Vec::new();
    config
        .try_serialize(&mut data)
        .expect("config should serialize");

    svm.set_account(
        Address::from(config_pda.to_bytes()),
        Account {
            lamports: u64::MAX / 2,
            data,
            owner: Address::from(program_id.to_bytes()),
            executable: false,
            rent_epoch: 0,
        },
    );

    config_pda
}
