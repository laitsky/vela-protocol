use anchor_lang::prelude::*;

#[account]
pub struct KeeperConfig {
    pub admin: Pubkey,              // 32 - authority to update config
    pub mode: KeeperMode,           // 1 - Centralized or TukTuk
    pub keeper_endpoint: [u8; 128], // 128 - URL for centralized, or task queue pubkey for TukTuk
    pub endpoint_len: u8,           // 1 - actual length of keeper_endpoint bytes
    pub keeper_authority: Pubkey,   // 32 - pubkey authorized to execute pulls (keeper wallet)
    pub bump: u8,                   // 1
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
    pub const SIZE: usize = 8 + 32 + 1 + 128 + 1 + 32 + 1; // 203 bytes

    pub fn endpoint(&self) -> &[u8] {
        &self.keeper_endpoint[..self.endpoint_len as usize]
    }

    pub fn endpoint_str(&self) -> Result<&str> {
        core::str::from_utf8(self.endpoint())
            .map_err(|_| anchor_lang::error::Error::from(ProgramError::InvalidAccountData))
    }
}
