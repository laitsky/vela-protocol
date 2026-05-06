use anchor_lang::{prelude::*, AccountDeserialize, AccountSerialize, Discriminator};

use crate::state::{BillingType, MandateStatus, VelaMandate, CURRENT_ACCOUNT_VERSION};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct VelaMandateV1 {
    pub subscriber: Pubkey,
    pub plan: Pubkey,
    pub merchant: Pubkey,
    pub amount: u64,
    pub frequency: u64,
    pub start_date: i64,
    pub expiry: i64,
    pub max_pulls: u64,
    pub pulls_executed: u64,
    pub next_payment_due: i64,
    pub last_pull_at: i64,
    pub last_billing_recorded_pull: u64,
    pub validation_request_nonce: u64,
    pub billing_request_nonce: u64,
    pub status: MandateStatus,
    pub bump: u8,
    pub billing_type: BillingType,
}

fn current_from_legacy_mandate(legacy: &VelaMandateV1) -> VelaMandate {
    VelaMandate {
        subscriber: legacy.subscriber,
        plan: legacy.plan,
        merchant: legacy.merchant,
        amount: legacy.amount,
        frequency: legacy.frequency,
        start_date: legacy.start_date,
        expiry: legacy.expiry,
        max_pulls: legacy.max_pulls,
        pulls_executed: legacy.pulls_executed,
        next_payment_due: legacy.next_payment_due,
        last_pull_at: legacy.last_pull_at,
        last_billing_recorded_pull: legacy.last_billing_recorded_pull,
        validation_request_nonce: legacy.validation_request_nonce,
        billing_request_nonce: legacy.billing_request_nonce,
        status: legacy.status.clone(),
        bump: legacy.bump,
        billing_type: legacy.billing_type.clone(),
        mandate_index: 0,
        version: CURRENT_ACCOUNT_VERSION,
        credit_balance: 0,
        pending_new_plan: Pubkey::default(),
        pending_effective_at: 0,
        pending_change_type: 0,
        pending_nonce_short: [0; 8],
        _reserved_v3: [0; 7],
    }
}

macro_rules! mandate_getter {
    ($name:ident, $field:ident, $ty:ty) => {
        pub fn $name(&self) -> $ty {
            match self {
                Self::Current(mandate) => mandate.$field,
                Self::Legacy(mandate) => mandate.$field,
            }
        }
    };
}

macro_rules! mandate_getter_clone {
    ($name:ident, $field:ident, $ty:ty) => {
        pub fn $name(&self) -> $ty {
            match self {
                Self::Current(mandate) => mandate.$field.clone(),
                Self::Legacy(mandate) => mandate.$field.clone(),
            }
        }
    };
}

macro_rules! mandate_setter {
    ($name:ident, $field:ident, $ty:ty) => {
        pub fn $name(&mut self, value: $ty) {
            match self {
                Self::Current(mandate) => mandate.$field = value,
                Self::Legacy(mandate) => mandate.$field = value,
            }
        }
    };
}

macro_rules! mandate_setter_clone {
    ($name:ident, $field:ident, $ty:ty) => {
        pub fn $name(&mut self, value: $ty) {
            match self {
                Self::Current(mandate) => mandate.$field = value.clone(),
                Self::Legacy(mandate) => mandate.$field = value,
            }
        }
    };
}

pub enum LoadedMandateAccount {
    Current(VelaMandate),
    Legacy(VelaMandateV1),
}

impl LoadedMandateAccount {
    pub fn into_current(self) -> VelaMandate {
        match self {
            Self::Current(current) => current,
            Self::Legacy(legacy) => current_from_legacy_mandate(&legacy),
        }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::Legacy(_))
    }

    mandate_getter!(subscriber, subscriber, Pubkey);
    mandate_getter!(plan, plan, Pubkey);
    mandate_getter!(merchant, merchant, Pubkey);
    mandate_getter!(amount, amount, u64);
    mandate_getter!(frequency, frequency, u64);
    mandate_getter!(expiry, expiry, i64);
    mandate_getter!(max_pulls, max_pulls, u64);
    mandate_getter!(pulls_executed, pulls_executed, u64);
    mandate_getter!(next_payment_due, next_payment_due, i64);
    mandate_getter!(last_billing_recorded_pull, last_billing_recorded_pull, u64);
    mandate_getter!(bump, bump, u8);

    pub fn mandate_index(&self) -> u64 {
        match self {
            Self::Current(mandate) => mandate.mandate_index,
            Self::Legacy(_) => 0,
        }
    }

    mandate_getter_clone!(status, status, MandateStatus);
    mandate_getter_clone!(billing_type, billing_type, BillingType);

    mandate_setter!(set_plan, plan, Pubkey);
    mandate_setter!(set_amount, amount, u64);
    mandate_setter!(set_frequency, frequency, u64);
    mandate_setter!(set_max_pulls, max_pulls, u64);
    mandate_setter!(set_pulls_executed, pulls_executed, u64);
    mandate_setter!(set_next_payment_due, next_payment_due, i64);
    mandate_setter!(set_last_pull_at, last_pull_at, i64);
    mandate_setter!(
        set_last_billing_recorded_pull,
        last_billing_recorded_pull,
        u64
    );
    mandate_setter_clone!(set_status, status, MandateStatus);
    mandate_setter_clone!(set_billing_type, billing_type, BillingType);
}

pub fn derive_v2_mandate_address(
    subscriber: &Pubkey,
    merchant: &Pubkey,
    mandate_index: u64,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            VelaMandate::SEED_PREFIX,
            subscriber.as_ref(),
            merchant.as_ref(),
            mandate_index.to_le_bytes().as_ref(),
        ],
        &crate::ID,
    )
    .0
}

pub fn derive_legacy_mandate_address(subscriber: &Pubkey, plan: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[VelaMandate::SEED_PREFIX, subscriber.as_ref(), plan.as_ref()],
        &crate::ID,
    )
    .0
}

pub fn load_mandate_account(mandate_info: &AccountInfo<'_>) -> Result<LoadedMandateAccount> {
    if *mandate_info.owner != crate::ID {
        return Err(ProgramError::IncorrectProgramId.into());
    }

    let data = mandate_info.try_borrow_data()?;
    if data.len() < VelaMandate::DISCRIMINATOR.len()
        || !data.starts_with(VelaMandate::DISCRIMINATOR)
    {
        return Err(ProgramError::InvalidAccountData.into());
    }

    // Try current layout first.
    {
        let mut slice: &[u8] = &data;
        if let Ok(mandate) = VelaMandate::try_deserialize(&mut slice) {
            if mandate.version == CURRENT_ACCOUNT_VERSION {
                return Ok(LoadedMandateAccount::Current(mandate));
            }
        }
    }

    // Fall back to legacy layout.
    let mut legacy_slice: &[u8] = &data[VelaMandate::DISCRIMINATOR.len()..];
    let legacy = VelaMandateV1::deserialize(&mut legacy_slice)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    if !legacy_slice.is_empty() && !has_only_zero_padding(legacy_slice) {
        return Err(ProgramError::InvalidAccountData.into());
    }
    Ok(LoadedMandateAccount::Legacy(legacy))
}

pub fn validate_loaded_mandate_address(
    mandate_key: &Pubkey,
    mandate: &LoadedMandateAccount,
) -> Result<()> {
    let expected_legacy = derive_legacy_mandate_address(&mandate.subscriber(), &mandate.plan());
    match mandate {
        LoadedMandateAccount::Legacy(_) => {
            require_keys_eq!(*mandate_key, expected_legacy);
        }
        LoadedMandateAccount::Current(_) => {
            let expected_v2 = derive_v2_mandate_address(
                &mandate.subscriber(),
                &mandate.merchant(),
                mandate.mandate_index(),
            );
            if *mandate_key != expected_v2 && *mandate_key != expected_legacy {
                return Err(ProgramError::InvalidSeeds.into());
            }
        }
    }
    Ok(())
}

pub fn write_mandate(
    mandate_info: &AccountInfo<'_>,
    mandate: &VelaMandate,
    legacy_layout: bool,
) -> Result<()> {
    let mut data = mandate_info.try_borrow_mut_data()?;
    data.fill(0);

    if legacy_layout {
        let legacy = VelaMandateV1 {
            subscriber: mandate.subscriber,
            plan: mandate.plan,
            merchant: mandate.merchant,
            amount: mandate.amount,
            frequency: mandate.frequency,
            start_date: mandate.start_date,
            expiry: mandate.expiry,
            max_pulls: mandate.max_pulls,
            pulls_executed: mandate.pulls_executed,
            next_payment_due: mandate.next_payment_due,
            last_pull_at: mandate.last_pull_at,
            last_billing_recorded_pull: mandate.last_billing_recorded_pull,
            validation_request_nonce: mandate.validation_request_nonce,
            billing_request_nonce: mandate.billing_request_nonce,
            status: mandate.status.clone(),
            bump: mandate.bump,
            billing_type: mandate.billing_type.clone(),
        };
        data[..VelaMandate::DISCRIMINATOR.len()].copy_from_slice(VelaMandate::DISCRIMINATOR);
        let mut body: &mut [u8] = &mut data[VelaMandate::DISCRIMINATOR.len()..];
        legacy.serialize(&mut body)?;
        return Ok(());
    }

    let mut slice: &mut [u8] = &mut data[..];
    mandate.try_serialize(&mut slice)?;
    Ok(())
}

pub fn write_mandate_account(
    mandate_info: &AccountInfo<'_>,
    mandate: &LoadedMandateAccount,
) -> Result<()> {
    match mandate {
        LoadedMandateAccount::Current(current) => write_mandate(mandate_info, current, false),
        LoadedMandateAccount::Legacy(legacy) => {
            let current = current_from_legacy_mandate(legacy);
            write_mandate(mandate_info, &current, true)
        }
    }
}

fn has_only_zero_padding(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}
