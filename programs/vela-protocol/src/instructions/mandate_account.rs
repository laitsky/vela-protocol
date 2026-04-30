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

pub enum LoadedMandateAccount {
    Current(VelaMandate),
    Legacy(VelaMandateV1),
}

impl LoadedMandateAccount {
    pub fn into_current(self) -> VelaMandate {
        match self {
            Self::Current(current) => current,
            Self::Legacy(legacy) => VelaMandate {
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
                status: legacy.status,
                bump: legacy.bump,
                billing_type: legacy.billing_type,
                mandate_index: 0,
                version: CURRENT_ACCOUNT_VERSION,
                credit_balance: 0,
                pending_new_plan: Pubkey::default(),
                pending_effective_at: 0,
                pending_change_type: 0,
                pending_nonce_short: [0; 8],
                _reserved_v3: [0; 7],
            },
        }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::Legacy(_))
    }

    pub fn subscriber(&self) -> Pubkey {
        match self {
            Self::Current(mandate) => mandate.subscriber,
            Self::Legacy(mandate) => mandate.subscriber,
        }
    }

    pub fn plan(&self) -> Pubkey {
        match self {
            Self::Current(mandate) => mandate.plan,
            Self::Legacy(mandate) => mandate.plan,
        }
    }

    pub fn merchant(&self) -> Pubkey {
        match self {
            Self::Current(mandate) => mandate.merchant,
            Self::Legacy(mandate) => mandate.merchant,
        }
    }

    pub fn amount(&self) -> u64 {
        match self {
            Self::Current(mandate) => mandate.amount,
            Self::Legacy(mandate) => mandate.amount,
        }
    }

    pub fn frequency(&self) -> u64 {
        match self {
            Self::Current(mandate) => mandate.frequency,
            Self::Legacy(mandate) => mandate.frequency,
        }
    }

    pub fn expiry(&self) -> i64 {
        match self {
            Self::Current(mandate) => mandate.expiry,
            Self::Legacy(mandate) => mandate.expiry,
        }
    }

    pub fn max_pulls(&self) -> u64 {
        match self {
            Self::Current(mandate) => mandate.max_pulls,
            Self::Legacy(mandate) => mandate.max_pulls,
        }
    }

    pub fn pulls_executed(&self) -> u64 {
        match self {
            Self::Current(mandate) => mandate.pulls_executed,
            Self::Legacy(mandate) => mandate.pulls_executed,
        }
    }

    pub fn next_payment_due(&self) -> i64 {
        match self {
            Self::Current(mandate) => mandate.next_payment_due,
            Self::Legacy(mandate) => mandate.next_payment_due,
        }
    }

    pub fn last_billing_recorded_pull(&self) -> u64 {
        match self {
            Self::Current(mandate) => mandate.last_billing_recorded_pull,
            Self::Legacy(mandate) => mandate.last_billing_recorded_pull,
        }
    }

    pub fn bump(&self) -> u8 {
        match self {
            Self::Current(mandate) => mandate.bump,
            Self::Legacy(mandate) => mandate.bump,
        }
    }

    pub fn mandate_index(&self) -> u64 {
        match self {
            Self::Current(mandate) => mandate.mandate_index,
            Self::Legacy(_) => 0,
        }
    }

    pub fn status(&self) -> MandateStatus {
        match self {
            Self::Current(mandate) => mandate.status.clone(),
            Self::Legacy(mandate) => mandate.status.clone(),
        }
    }

    pub fn billing_type(&self) -> BillingType {
        match self {
            Self::Current(mandate) => mandate.billing_type.clone(),
            Self::Legacy(mandate) => mandate.billing_type.clone(),
        }
    }

    pub fn set_plan(&mut self, value: Pubkey) {
        match self {
            Self::Current(mandate) => mandate.plan = value,
            Self::Legacy(mandate) => mandate.plan = value,
        }
    }

    pub fn set_amount(&mut self, value: u64) {
        match self {
            Self::Current(mandate) => mandate.amount = value,
            Self::Legacy(mandate) => mandate.amount = value,
        }
    }

    pub fn set_frequency(&mut self, value: u64) {
        match self {
            Self::Current(mandate) => mandate.frequency = value,
            Self::Legacy(mandate) => mandate.frequency = value,
        }
    }

    pub fn set_max_pulls(&mut self, value: u64) {
        match self {
            Self::Current(mandate) => mandate.max_pulls = value,
            Self::Legacy(mandate) => mandate.max_pulls = value,
        }
    }

    pub fn set_pulls_executed(&mut self, value: u64) {
        match self {
            Self::Current(mandate) => mandate.pulls_executed = value,
            Self::Legacy(mandate) => mandate.pulls_executed = value,
        }
    }

    pub fn set_next_payment_due(&mut self, value: i64) {
        match self {
            Self::Current(mandate) => mandate.next_payment_due = value,
            Self::Legacy(mandate) => mandate.next_payment_due = value,
        }
    }

    pub fn set_last_pull_at(&mut self, value: i64) {
        match self {
            Self::Current(mandate) => mandate.last_pull_at = value,
            Self::Legacy(mandate) => mandate.last_pull_at = value,
        }
    }

    pub fn set_last_billing_recorded_pull(&mut self, value: u64) {
        match self {
            Self::Current(mandate) => mandate.last_billing_recorded_pull = value,
            Self::Legacy(mandate) => mandate.last_billing_recorded_pull = value,
        }
    }

    pub fn set_status(&mut self, value: MandateStatus) {
        match self {
            Self::Current(mandate) => mandate.status = value.clone(),
            Self::Legacy(mandate) => mandate.status = value,
        }
    }

    pub fn set_billing_type(&mut self, value: BillingType) {
        match self {
            Self::Current(mandate) => mandate.billing_type = value.clone(),
            Self::Legacy(mandate) => mandate.billing_type = value,
        }
    }
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
            let current = VelaMandate {
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
            };
            write_mandate(mandate_info, &current, true)
        }
    }
}

fn has_only_zero_padding(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}
