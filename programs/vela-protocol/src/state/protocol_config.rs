use anchor_lang::prelude::*;

#[account]
pub struct ProtocolConfig {
    pub admin: Pubkey,               // 32 - upgrade authority
    pub cluster_pubkey: Pubkey,      // 32 - Arcium cluster to use
    pub cluster_type: ClusterType,   // 1 - Cerberus or Manticore
    pub cluster_offset: u64,         // 8 - devnet: 456
    pub wrapped_usdc_mint: Pubkey,   // 32 - Token-2022 wrapped USDC mint (set by init_wrapped_mint)
    pub wrapping_vault: Pubkey,      // 32 - SPL USDC vault ATA owned by mint authority PDA
    pub bump: u8,                    // 1
}

impl ProtocolConfig {
    pub const SEED_PREFIX: &'static [u8] = b"config";
    // Seeds: [b"config"]
    // Singleton PDA -- one per program deployment
    pub const SIZE: usize = 8 + 32 + 32 + 1 + 8 + 32 + 32 + 1; // 146 bytes
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum ClusterType {
    Cerberus,  // permissioned, for devnet (D-11)
    Manticore, // permissionless, for mainnet
}
