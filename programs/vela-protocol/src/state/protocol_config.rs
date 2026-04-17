use anchor_lang::prelude::*;

use super::CURRENT_ACCOUNT_VERSION;

pub const PROTOCOL_CONFIG_RESERVED_BYTES: usize = 32;

#[account]
pub struct ProtocolConfig {
    pub admin: Pubkey,                    // 32 - upgrade authority
    pub cluster_pubkey: Pubkey,           // 32 - Arcium cluster to use
    pub cluster_type: ClusterType,        // 1 - Cerberus or Manticore
    pub cluster_offset: u64,              // 8 - devnet: 456
    pub wrapped_usdc_mint: Pubkey, // 32 - Token-2022 wrapped USDC mint (set by init_wrapped_mint)
    pub wrapping_vault: Pubkey,    // 32 - SPL USDC vault ATA owned by mint authority PDA
    pub paused: bool,              // 1 - emergency pause flag
    pub paused_at: i64,            // 8 - unix timestamp when paused, 0 when not paused
    pub transfer_hook_program_id: Pubkey, // 32 - dynamic transfer hook program ID
    pub bump: u8,                  // 1
    pub version: u8,               // 1 - schema version
    pub _reserved: [u8; PROTOCOL_CONFIG_RESERVED_BYTES],
}

impl ProtocolConfig {
    pub const SEED_PREFIX: &'static [u8] = b"config";
    // Seeds: [b"config"]
    // Singleton PDA -- one per program deployment
    // SIZE = 8 + 32 + 32 + 1 + 8 + 32 + 32 + 1 + 8 + 32 + 1 + 1 + 32 = 220 bytes
    pub const SIZE: usize =
        8 + 32 + 32 + 1 + 8 + 32 + 32 + 1 + 8 + 32 + 1 + 1 + PROTOCOL_CONFIG_RESERVED_BYTES;

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
            transfer_hook_program_id: Pubkey::default(),
            bump,
            version: CURRENT_ACCOUNT_VERSION,
            _reserved: [0; PROTOCOL_CONFIG_RESERVED_BYTES],
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum ClusterType {
    Cerberus,  // permissioned, for devnet (D-11)
    Manticore, // permissionless, for mainnet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_config_size_remains_220_bytes() {
        assert_eq!(ProtocolConfig::SIZE, 220);
    }

    #[test]
    fn new_defaults_transfer_hook_program_id() {
        let config = ProtocolConfig::new(
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            ClusterType::Cerberus,
            456,
            1,
        );

        assert_eq!(config.transfer_hook_program_id, Pubkey::default());
        assert_eq!(config._reserved, [0; PROTOCOL_CONFIG_RESERVED_BYTES]);
    }
}
