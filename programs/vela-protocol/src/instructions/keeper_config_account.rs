use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, program::invoke_signed, system_instruction},
    AccountSerialize, Discriminator,
};

use crate::state::{KeeperConfig, KeeperMode, ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub struct KeeperConfigV1 {
    pub admin: Pubkey,
    pub mode: KeeperMode,
    pub keeper_endpoint: [u8; 128],
    pub endpoint_len: u8,
    pub keeper_authority: Pubkey,
    pub bump: u8,
}

pub enum LoadedKeeperConfig {
    Legacy(KeeperConfigV1),
    Current(KeeperConfig),
}

impl LoadedKeeperConfig {
    pub fn bump(&self) -> u8 {
        match self {
            Self::Legacy(config) => config.bump,
            Self::Current(config) => config.bump,
        }
    }

    pub fn keeper_authority(&self) -> Pubkey {
        match self {
            Self::Legacy(config) => config.keeper_authority,
            Self::Current(config) => config.keeper_authority,
        }
    }

    pub fn into_current(self) -> KeeperConfig {
        match self {
            Self::Legacy(legacy) => KeeperConfig {
                admin: legacy.admin,
                mode: legacy.mode,
                keeper_endpoint: legacy.keeper_endpoint,
                endpoint_len: legacy.endpoint_len,
                keeper_authority: legacy.keeper_authority,
                bump: legacy.bump,
                version: CURRENT_ACCOUNT_VERSION,
                _reserved: [0; ACCOUNT_RESERVED_BYTES],
            },
            Self::Current(current) => current,
        }
    }
}

pub fn load_keeper_config(config_info: &AccountInfo<'_>) -> Result<LoadedKeeperConfig> {
    validate_keeper_config_address(config_info.key)?;

    if *config_info.owner != crate::ID {
        return Err(ProgramError::IncorrectProgramId.into());
    }

    let data = config_info.try_borrow_data()?;
    if data.len() < KeeperConfig::DISCRIMINATOR.len()
        || !data.starts_with(&KeeperConfig::DISCRIMINATOR)
    {
        return Err(ProgramError::InvalidAccountData.into());
    }

    let mut slice: &[u8] = &data;
    if let Ok(config) = KeeperConfig::try_deserialize(&mut slice) {
        if config.version != CURRENT_ACCOUNT_VERSION {
            return Err(ProgramError::InvalidAccountData.into());
        }
        return Ok(LoadedKeeperConfig::Current(config));
    }

    let mut legacy_slice: &[u8] = &data[KeeperConfig::DISCRIMINATOR.len()..];
    let legacy = KeeperConfigV1::deserialize(&mut legacy_slice)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    if !legacy_slice.is_empty() && !has_only_zero_padding(legacy_slice) {
        return Err(ProgramError::InvalidAccountData.into());
    }
    Ok(LoadedKeeperConfig::Legacy(legacy))
}

pub fn upgrade_keeper_config<'info>(
    payer: &AccountInfo<'info>,
    config_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<KeeperConfig> {
    let mut config = load_keeper_config(config_info)?.into_current();

    resize_keeper_config_account(payer, config_info, system_program)?;

    config.version = CURRENT_ACCOUNT_VERSION;
    config._reserved = [0; ACCOUNT_RESERVED_BYTES];
    write_keeper_config(config_info, &config)?;

    Ok(config)
}

pub fn initialize_keeper_config<'info>(
    payer: &AccountInfo<'info>,
    config_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    admin: Pubkey,
    mode: KeeperMode,
    keeper_endpoint: Vec<u8>,
    keeper_authority: Pubkey,
    bump: u8,
) -> Result<KeeperConfig> {
    validate_keeper_config_address(config_info.key)?;

    if !config_info.data_is_empty() {
        return Err(ProgramError::AccountAlreadyInitialized.into());
    }

    let rent = Rent::get()?;
    let lamports = rent.minimum_balance(KeeperConfig::SIZE);
    let signer_seeds: &[&[u8]] = &[KeeperConfig::SEED_PREFIX, &[bump]];

    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            config_info.key,
            lamports,
            KeeperConfig::SIZE as u64,
            &crate::ID,
        ),
        &[payer.clone(), config_info.clone(), system_program.clone()],
        &[signer_seeds],
    )?;
    let config = KeeperConfig::new(admin, mode, keeper_endpoint, keeper_authority, bump);
    write_keeper_config(config_info, &config)?;

    Ok(config)
}

pub fn write_keeper_config(config_info: &AccountInfo<'_>, config: &KeeperConfig) -> Result<()> {
    let mut data = config_info.try_borrow_mut_data()?;
    data.fill(0);
    let mut slice: &mut [u8] = &mut data[..];
    config.try_serialize(&mut slice)?;
    Ok(())
}

fn has_only_zero_padding(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

fn resize_keeper_config_account<'info>(
    payer: &AccountInfo<'info>,
    config_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<()> {
    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(KeeperConfig::SIZE);
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

    if config_info.data_len() < KeeperConfig::SIZE {
        config_info.resize(KeeperConfig::SIZE)?;
    }

    Ok(())
}

fn validate_keeper_config_address(config: &Pubkey) -> Result<()> {
    let (expected, _) = Pubkey::find_program_address(&[KeeperConfig::SEED_PREFIX], &crate::ID);
    if *config != expected {
        return Err(ProgramError::InvalidSeeds.into());
    }

    Ok(())
}
