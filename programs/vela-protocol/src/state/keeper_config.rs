use anchor_lang::prelude::*;

use super::{ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION};

#[account]
pub struct KeeperConfig {
    pub admin: Pubkey,              // 32 - authority to update config
    pub mode: KeeperMode,           // 1 - Centralized or TukTuk
    pub keeper_endpoint: [u8; 128], // 128 - URL for centralized, or task queue pubkey for TukTuk
    pub endpoint_len: u8,           // 1 - actual length of keeper_endpoint bytes
    pub keeper_authority: Pubkey,   // 32 - pubkey authorized to execute pulls (keeper wallet)
    pub bump: u8,                   // 1
    pub version: u8,                // 1 - schema version
    pub _reserved: [u8; ACCOUNT_RESERVED_BYTES],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum KeeperMode {
    Centralized,
    TukTuk,
}

impl KeeperConfig {
    pub const SEED_PREFIX: &'static [u8] = b"keeper-config";
    // Seeds: [b"keeper-config"]
    // Singleton PDA -- one per program deployment
    pub const SIZE: usize = 8 + 32 + 1 + 128 + 1 + 32 + 1 + 1 + ACCOUNT_RESERVED_BYTES;

    pub fn new(
        admin: Pubkey,
        mode: KeeperMode,
        keeper_endpoint: Vec<u8>,
        keeper_authority: Pubkey,
        bump: u8,
    ) -> Self {
        let mut endpoint = [0u8; 128];
        endpoint[..keeper_endpoint.len()].copy_from_slice(&keeper_endpoint);

        Self {
            admin,
            mode,
            keeper_endpoint: endpoint,
            endpoint_len: keeper_endpoint.len() as u8,
            keeper_authority,
            bump,
            version: CURRENT_ACCOUNT_VERSION,
            _reserved: [0; ACCOUNT_RESERVED_BYTES],
        }
    }

    pub fn endpoint(&self) -> &[u8] {
        let len = (self.endpoint_len as usize).min(self.keeper_endpoint.len());
        &self.keeper_endpoint[..len]
    }

    pub fn endpoint_str(&self) -> Result<&str> {
        core::str::from_utf8(self.endpoint())
            .map_err(|_| anchor_lang::error::Error::from(ProgramError::InvalidAccountData))
    }
}
