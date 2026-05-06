use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, program::invoke_signed, system_instruction},
    AccountSerialize, Discriminator,
};

use crate::state::{
    MerchantState, ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION, LEGACY_ACCOUNT_VERSION,
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MerchantStateV1 {
    pub merchant: Pubkey,
    pub plan_count: u64,
    pub bump: u8,
}

pub enum LoadedMerchantState {
    Legacy(MerchantStateV1),
    Current(MerchantState),
}

impl LoadedMerchantState {
    pub fn into_current(self) -> MerchantState {
        match self {
            Self::Legacy(legacy) => MerchantState {
                merchant: legacy.merchant,
                plan_count: legacy.plan_count,
                bump: legacy.bump,
                credential_mint: Pubkey::default(),
                mandate_counter: 0,
                stream_mandate_counter: 0,
                version: CURRENT_ACCOUNT_VERSION,
                _reserved: [0; ACCOUNT_RESERVED_BYTES - 8],
            },
            Self::Current(current) => current,
        }
    }
}

pub fn load_merchant_state(
    merchant_state_info: &AccountInfo<'_>,
    merchant: &Pubkey,
) -> Result<LoadedMerchantState> {
    validate_merchant_state_address(merchant_state_info.key, merchant)?;

    if *merchant_state_info.owner != crate::ID {
        return Err(ProgramError::IncorrectProgramId.into());
    }

    let data = merchant_state_info.try_borrow_data()?;
    if data.len() < MerchantState::DISCRIMINATOR.len()
        || !data.starts_with(MerchantState::DISCRIMINATOR)
    {
        return Err(ProgramError::InvalidAccountData.into());
    }

    let mut slice: &[u8] = &data;
    if let Ok(state) = MerchantState::try_deserialize(&mut slice) {
        return Ok(LoadedMerchantState::Current(state));
    }

    let mut legacy_slice: &[u8] = &data[MerchantState::DISCRIMINATOR.len()..];
    let legacy = MerchantStateV1::deserialize(&mut legacy_slice)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(LoadedMerchantState::Legacy(legacy))
}

pub fn ensure_merchant_state<'info>(
    payer: &AccountInfo<'info>,
    merchant_state_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    rent: &Rent,
    merchant: &Pubkey,
    bump: u8,
) -> Result<MerchantState> {
    validate_merchant_state_address(merchant_state_info.key, merchant)?;

    if merchant_state_info.data_is_empty() {
        create_merchant_state_account(
            payer,
            merchant_state_info,
            system_program,
            rent,
            merchant,
            bump,
        )?;

        let state = MerchantState::new(*merchant, bump, 0);
        write_merchant_state(merchant_state_info, &state)?;
        return Ok(state);
    }

    let mut state = load_merchant_state(merchant_state_info, merchant)?.into_current();
    if state.merchant != Pubkey::default() && state.merchant != *merchant {
        return Err(ProgramError::InvalidAccountData.into());
    }

    resize_merchant_state_account(payer, merchant_state_info, system_program, rent)?;

    state.merchant = *merchant;
    state.bump = bump;
    state.version = CURRENT_ACCOUNT_VERSION;
    if state.version == LEGACY_ACCOUNT_VERSION {
        state.version = CURRENT_ACCOUNT_VERSION;
    }
    state._reserved = [0; ACCOUNT_RESERVED_BYTES - 8];
    write_merchant_state(merchant_state_info, &state)?;

    Ok(state)
}

pub fn write_merchant_state(
    merchant_state_info: &AccountInfo<'_>,
    state: &MerchantState,
) -> Result<()> {
    let mut data = merchant_state_info.try_borrow_mut_data()?;
    let mut slice: &mut [u8] = &mut data[..];
    state.try_serialize(&mut slice)?;
    Ok(())
}

fn create_merchant_state_account<'info>(
    payer: &AccountInfo<'info>,
    merchant_state_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    rent: &Rent,
    merchant: &Pubkey,
    bump: u8,
) -> Result<()> {
    let lamports = rent.minimum_balance(MerchantState::SIZE);
    let signer_seeds: &[&[u8]] = &[MerchantState::SEED_PREFIX, merchant.as_ref(), &[bump]];

    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            merchant_state_info.key,
            lamports,
            MerchantState::SIZE as u64,
            &crate::ID,
        ),
        &[
            payer.clone(),
            merchant_state_info.clone(),
            system_program.clone(),
        ],
        &[signer_seeds],
    )?;

    Ok(())
}

fn resize_merchant_state_account<'info>(
    payer: &AccountInfo<'info>,
    merchant_state_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    rent: &Rent,
) -> Result<()> {
    if *merchant_state_info.owner != crate::ID {
        return Err(ProgramError::IncorrectProgramId.into());
    }

    let required_lamports = rent.minimum_balance(MerchantState::SIZE);
    let current_lamports = merchant_state_info.lamports();
    if current_lamports < required_lamports {
        invoke(
            &system_instruction::transfer(
                payer.key,
                merchant_state_info.key,
                required_lamports - current_lamports,
            ),
            &[
                payer.clone(),
                merchant_state_info.clone(),
                system_program.clone(),
            ],
        )?;
    }

    if merchant_state_info.data_len() < MerchantState::SIZE {
        merchant_state_info.resize(MerchantState::SIZE)?;
    }

    Ok(())
}

pub fn validate_merchant_state_address(merchant_state: &Pubkey, merchant: &Pubkey) -> Result<()> {
    let (expected, _) =
        Pubkey::find_program_address(&[MerchantState::SEED_PREFIX, merchant.as_ref()], &crate::ID);

    if *merchant_state != expected {
        return Err(ProgramError::InvalidSeeds.into());
    }

    Ok(())
}

/// Centralized resolver for the merchant credential mint.
///
/// Resolves from MerchantState first (D-10, D-12). If MerchantState has a non-default
/// `credential_mint`, returns it. Otherwise falls back to the provided plan credential mint
/// for backward compatibility during migration (CRED-05).
///
/// Later plans will call this from `subscribe`, `cancel`, and `admin_cancel` instead of
/// directly reading from the plan account.
pub fn resolve_merchant_credential_mint(
    merchant_state_info: &AccountInfo<'_>,
    merchant: &Pubkey,
    plan_credential_mint: &Pubkey,
) -> Result<Pubkey> {
    // If the account doesn't exist or isn't owned by our program, fall back to plan credential
    if merchant_state_info.data_is_empty() || *merchant_state_info.owner != crate::ID {
        return Ok(*plan_credential_mint);
    }

    // Try to load the merchant state
    match load_merchant_state(merchant_state_info, merchant) {
        Ok(loaded) => {
            let state = loaded.into_current();
            if state.credential_mint != Pubkey::default() {
                Ok(state.credential_mint)
            } else {
                Ok(*plan_credential_mint)
            }
        }
        Err(_) => Ok(*plan_credential_mint),
    }
}
