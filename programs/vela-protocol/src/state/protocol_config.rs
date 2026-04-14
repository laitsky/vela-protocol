use anchor_lang::prelude::*;

use super::{ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION};

#[account]
pub struct ProtocolConfig {
    pub admin: Pubkey,               // 32 - upgrade authority
    pub cluster_pubkey: Pubkey,      // 32 - Arcium cluster to use
    pub cluster_type: ClusterType,   // 1 - Cerberus or Manticore
    pub cluster_offset: u64,         // 8 - devnet: 456
    pub wrapped_usdc_mint: Pubkey,   // 32 - Token-2022 wrapped USDC mint (set by init_wrapped_mint)
    pub wrapping_vault: Pubkey,      // 32 - SPL USDC vault ATA owned by mint authority PDA
    pub paused: bool,               // 1 - emergency pause flag
    pub paused_at: i64,             // 8 - unix timestamp when paused, 0 when not paused
    pub bump: u8,                    // 1
    pub version: u8,                 // 1 - schema version
    pub _reserved: [u8; ACCOUNT_RESERVED_BYTES],
}

impl ProtocolConfig {
    pub const SEED_PREFIX: &'static [u8] = b"config";
    // Seeds: [b"config"]
    // Singleton PDA -- one per program deployment
    // SIZE = 8 + 32 + 32 + 1 + 8 + 32 + 32 + 1 + 8 + 1 + 1 + 64 = 220 bytes
    pub const SIZE: usize = 8 + 32 + 32 + 1 + 8 + 32 + 32 + 1 + 8 + 1 + 1 + ACCOUNT_RESERVED_BYTES;

    pub fn new(
        admin: Pubkey,
        cluster_pubkey: Pubkey,
        cluster_type: ClusterType,
        cluster_offset: u64,
        bump: u8,
    ) -> Self {
        Self {
            admin,
            cluster_pubkey,
            cluster_type,
            cluster_offset,
            wrapped_usdc_mint: Pubkey::default(),
            wrapping_vault: Pubkey::default(),
            paused: false,
            paused_at: 0,
            bump,
            version: CURRENT_ACCOUNT_VERSION,
            _reserved: [0; ACCOUNT_RESERVED_BYTES],
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum ClusterType {
    Cerberus,  // permissioned, for devnet (D-11)
    Manticore, // permissionless, for mainnet
}
