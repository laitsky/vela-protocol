use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction},
    AccountSerialize, Discriminator,
};

use crate::{
    errors::VelaError,
    state::{ClusterType, ProtocolConfig, CURRENT_ACCOUNT_VERSION, PROTOCOL_CONFIG_RESERVED_BYTES},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProtocolConfigV1 {
    pub admin: Pubkey,
    pub cluster_pubkey: Pubkey,
    pub cluster_type: ClusterType,
    pub cluster_offset: u64,
    pub wrapped_usdc_mint: Pubkey,
    pub wrapping_vault: Pubkey,
    pub paused: bool,
    pub paused_at: i64,
    pub bump: u8,
}

pub enum LoadedProtocolConfig {
    Legacy(ProtocolConfigV1),
    Current(ProtocolConfig),
}

impl LoadedProtocolConfig {
    pub fn admin(&self) -> Pubkey {
        match self {
            Self::Legacy(config) => config.admin,
            Self::Current(config) => config.admin,
        }
    }

    pub fn bump(&self) -> u8 {
        match self {
            Self::Legacy(config) => config.bump,
            Self::Current(config) => config.bump,
        }
    }

    pub fn paused(&self) -> bool {
        match self {
            Self::Legacy(config) => config.paused,
            Self::Current(config) => config.paused,
        }
    }

    pub fn wrapped_usdc_mint(&self) -> Pubkey {
        match self {
            Self::Legacy(config) => config.wrapped_usdc_mint,
            Self::Current(config) => config.wrapped_usdc_mint,
        }
    }

    pub fn wrapping_vault(&self) -> Pubkey {
        match self {
            Self::Legacy(config) => config.wrapping_vault,
            Self::Current(config) => config.wrapping_vault,
        }
    }

    pub fn cluster_pubkey(&self) -> Pubkey {
        match self {
            Self::Legacy(config) => config.cluster_pubkey,
            Self::Current(config) => config.cluster_pubkey,
        }
    }

    pub fn cluster_offset(&self) -> u64 {
        match self {
            Self::Legacy(config) => config.cluster_offset,
            Self::Current(config) => config.cluster_offset,
        }
    }

    pub fn transfer_hook_program_id(&self) -> Pubkey {
        match self {
            Self::Legacy(_) => Pubkey::default(),
            Self::Current(config) => config.transfer_hook_program_id,
        }
    }

    pub fn into_current(self) -> ProtocolConfig {
        match self {
            Self::Legacy(legacy) => ProtocolConfig {
                admin: legacy.admin,
                cluster_pubkey: legacy.cluster_pubkey,
                cluster_type: legacy.cluster_type,
                cluster_offset: legacy.cluster_offset,
                wrapped_usdc_mint: legacy.wrapped_usdc_mint,
                wrapping_vault: legacy.wrapping_vault,
                paused: legacy.paused,
                paused_at: legacy.paused_at,
                transfer_hook_program_id: Pubkey::default(),
                bump: legacy.bump,
                version: CURRENT_ACCOUNT_VERSION,
                _reserved: [0; PROTOCOL_CONFIG_RESERVED_BYTES],
            },
            Self::Current(current) => current,
        }
    }
}

pub fn load_protocol_config(config_info: &AccountInfo<'_>) -> Result<LoadedProtocolConfig> {
    validate_protocol_config_address(config_info.key)?;

    if *config_info.owner != crate::ID {
        return Err(error!(VelaError::InvalidProtocolConfig));
    }

    let data = config_info.try_borrow_data()?;
    if data.len() < ProtocolConfig::DISCRIMINATOR.len()
        || !data.starts_with(&ProtocolConfig::DISCRIMINATOR)
    {
        return Err(error!(VelaError::InvalidProtocolConfig));
    }

    let mut slice: &[u8] = &data;
    if let Ok(config) = ProtocolConfig::try_deserialize(&mut slice) {
        if config.version != CURRENT_ACCOUNT_VERSION {
            return Err(ProgramError::InvalidAccountData.into());
        }
        return Ok(LoadedProtocolConfig::Current(config));
    }

    let mut legacy_slice: &[u8] = &data[ProtocolConfig::DISCRIMINATOR.len()..];
    let legacy = ProtocolConfigV1::deserialize(&mut legacy_slice)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    if !legacy_slice.is_empty() && !has_only_zero_padding(legacy_slice) {
        return Err(ProgramError::InvalidAccountData.into());
    }
    Ok(LoadedProtocolConfig::Legacy(legacy))
}

pub fn upgrade_protocol_config<'info>(
    payer: &AccountInfo<'info>,
    config_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<ProtocolConfig> {
    let mut config = load_protocol_config(config_info)?.into_current();

    resize_protocol_config_account(payer, config_info, system_program)?;

    config.version = CURRENT_ACCOUNT_VERSION;
    config._reserved = [0; PROTOCOL_CONFIG_RESERVED_BYTES];
    write_protocol_config(config_info, &config)?;

    Ok(config)
}

pub fn write_protocol_config(config_info: &AccountInfo<'_>, config: &ProtocolConfig) -> Result<()> {
    let mut data = config_info.try_borrow_mut_data()?;
    data.fill(0);
    let mut slice: &mut [u8] = &mut data[..];
    config.try_serialize(&mut slice)?;
    Ok(())
}

fn has_only_zero_padding(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

fn resize_protocol_config_account<'info>(
    payer: &AccountInfo<'info>,
    config_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<()> {
    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(ProtocolConfig::SIZE);
    let current_lamports = config_info.lamports();
    if current_lamports < required_lamports {
        invoke(
            &system_instruction::transfer(
                payer.key,
                config_info.key,
                required_lamports - current_lamports,
            ),
            &[payer.clone(), config_info.clone(), system_program.clone()],
        )?;
    }

    if config_info.data_len() < ProtocolConfig::SIZE {
        config_info.resize(ProtocolConfig::SIZE)?;
    }

    Ok(())
}

fn validate_protocol_config_address(config: &Pubkey) -> Result<()> {
    let (expected, _) = Pubkey::find_program_address(&[ProtocolConfig::SEED_PREFIX], &crate::ID);
    if *config != expected {
        return Err(ProgramError::InvalidSeeds.into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_upgrade_defaults_transfer_hook_program_id() {
        let legacy = ProtocolConfigV1 {
            admin: Pubkey::new_unique(),
            cluster_pubkey: Pubkey::new_unique(),
            cluster_type: ClusterType::Cerberus,
            cluster_offset: 456,
            wrapped_usdc_mint: Pubkey::new_unique(),
            wrapping_vault: Pubkey::new_unique(),
            paused: false,
            paused_at: 0,
            bump: 7,
        };

        let upgraded = LoadedProtocolConfig::Legacy(legacy.clone()).into_current();

        assert_eq!(upgraded.admin, legacy.admin);
        assert_eq!(upgraded.cluster_pubkey, legacy.cluster_pubkey);
        assert_eq!(upgraded.transfer_hook_program_id, Pubkey::default());
        assert_eq!(upgraded.version, CURRENT_ACCOUNT_VERSION);
        assert_eq!(upgraded._reserved, [0; PROTOCOL_CONFIG_RESERVED_BYTES]);
    }

    #[test]
    fn current_accessor_returns_transfer_hook_program_id() {
        let transfer_hook_program_id = Pubkey::new_unique();
        let loaded = LoadedProtocolConfig::Current(ProtocolConfig {
            admin: Pubkey::new_unique(),
            cluster_pubkey: Pubkey::new_unique(),
            cluster_type: ClusterType::Cerberus,
            cluster_offset: 456,
            wrapped_usdc_mint: Pubkey::new_unique(),
            wrapping_vault: Pubkey::new_unique(),
            paused: false,
            paused_at: 0,
            transfer_hook_program_id,
            bump: 1,
            version: CURRENT_ACCOUNT_VERSION,
            _reserved: [0; PROTOCOL_CONFIG_RESERVED_BYTES],
        });

        assert_eq!(loaded.transfer_hook_program_id(), transfer_hook_program_id);
    }
}
