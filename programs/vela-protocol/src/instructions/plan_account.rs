use anchor_lang::{prelude::*, AccountDeserialize, AccountSerialize, Discriminator};

use crate::{
    errors::VelaError,
    state::{BillingType, PlanStatus, VelaPlan, UsagePlan, PricingTier, CURRENT_ACCOUNT_VERSION},
};

/// Legacy V1 flat plan layout (no version/_reserved fields).
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct VelaPlanV1 {
    pub merchant: Pubkey,
    pub plan_id: u64,
    pub amount: u64,
    pub frequency: u64,
    pub trial_period: u64,
    pub max_pulls: u64,
    pub status: PlanStatus,
    pub credential_mint: Pubkey,
    pub bump: u8,
}

/// Legacy V1 usage plan layout (no version/_reserved fields).
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UsagePlanV1 {
    pub merchant: Pubkey,
    pub plan_id: u64,
    pub unit_name: [u8; 32],
    pub tiers: [PricingTier; 5],
    pub tier_count: u8,
    pub max_charge_per_period: u64,
    pub settlement_frequency: u64,
    pub credential_mint: Pubkey,
    pub status: PlanStatus,
    pub bump: u8,
}

pub enum LoadedPlanAccount {
    Flat(VelaPlan),
    Usage(UsagePlan),
    LegacyFlat(VelaPlanV1),
    LegacyUsage(UsagePlanV1),
}

impl LoadedPlanAccount {
    pub fn billing_type(&self) -> BillingType {
        match self {
            Self::Flat(_) | Self::LegacyFlat(_) => BillingType::Flat,
            Self::Usage(_) | Self::LegacyUsage(_) => BillingType::Usage,
        }
    }

    pub fn merchant(&self) -> Pubkey {
        match self {
            Self::Flat(plan) => plan.merchant,
            Self::Usage(plan) => plan.merchant,
            Self::LegacyFlat(plan) => plan.merchant,
            Self::LegacyUsage(plan) => plan.merchant,
        }
    }

    pub fn credential_mint(&self) -> Pubkey {
        match self {
            Self::Flat(plan) => plan.credential_mint,
            Self::Usage(plan) => plan.credential_mint,
            Self::LegacyFlat(plan) => plan.credential_mint,
            Self::LegacyUsage(plan) => plan.credential_mint,
        }
    }

    pub fn status(&self) -> &PlanStatus {
        match self {
            Self::Flat(plan) => &plan.status,
            Self::Usage(plan) => &plan.status,
            Self::LegacyFlat(plan) => &plan.status,
            Self::LegacyUsage(plan) => &plan.status,
        }
    }

    pub fn mandate_amount(&self) -> u64 {
        match self {
            Self::Flat(plan) => plan.amount,
            Self::Usage(plan) => plan.max_charge_per_period,
            Self::LegacyFlat(plan) => plan.amount,
            Self::LegacyUsage(plan) => plan.max_charge_per_period,
        }
    }

    pub fn mandate_frequency(&self) -> u64 {
        match self {
            Self::Flat(plan) => plan.frequency,
            Self::Usage(plan) => plan.settlement_frequency,
            Self::LegacyFlat(plan) => plan.frequency,
            Self::LegacyUsage(plan) => plan.settlement_frequency,
        }
    }

    pub fn max_pulls(&self) -> u64 {
        match self {
            Self::Flat(plan) => plan.max_pulls,
            Self::Usage(_) => u64::MAX,
            Self::LegacyFlat(plan) => plan.max_pulls,
            Self::LegacyUsage(_) => u64::MAX,
        }
    }

    pub fn plan_id(&self) -> u64 {
        match self {
            Self::Flat(plan) => plan.plan_id,
            Self::Usage(plan) => plan.plan_id,
            Self::LegacyFlat(plan) => plan.plan_id,
            Self::LegacyUsage(plan) => plan.plan_id,
        }
    }

    pub fn bump(&self) -> u8 {
        match self {
            Self::Flat(plan) => plan.bump,
            Self::Usage(plan) => plan.bump,
            Self::LegacyFlat(plan) => plan.bump,
            Self::LegacyUsage(plan) => plan.bump,
        }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::LegacyFlat(_) | Self::LegacyUsage(_))
    }

    pub fn is_flat(&self) -> bool {
        matches!(self, Self::Flat(_) | Self::LegacyFlat(_))
    }

    pub fn expiry(&self, start_timestamp: i64) -> Result<i64> {
        match self {
            Self::Flat(plan) => Self::compute_flat_expiry(
                plan.trial_period,
                plan.frequency,
                plan.max_pulls,
                start_timestamp,
            ),
            Self::LegacyFlat(plan) => Self::compute_flat_expiry(
                plan.trial_period,
                plan.frequency,
                plan.max_pulls,
                start_timestamp,
            ),
            Self::Usage(_) | Self::LegacyUsage(_) => Ok(0),
        }
    }

    pub fn initial_next_payment_due(&self, start_timestamp: i64) -> Result<i64> {
        match self {
            Self::Flat(plan) => {
                let delay = if plan.trial_period > 0 {
                    plan.trial_period
                } else {
                    plan.frequency
                };
                Self::add_delay(start_timestamp, delay)
            }
            Self::LegacyFlat(plan) => {
                let delay = if plan.trial_period > 0 {
                    plan.trial_period
                } else {
                    plan.frequency
                };
                Self::add_delay(start_timestamp, delay)
            }
            Self::Usage(plan) => {
                Self::add_delay(start_timestamp, plan.settlement_frequency)
            }
            Self::LegacyUsage(plan) => {
                Self::add_delay(start_timestamp, plan.settlement_frequency)
            }
        }
    }

    fn compute_flat_expiry(
        trial_period: u64,
        frequency: u64,
        max_pulls: u64,
        start_timestamp: i64,
    ) -> Result<i64> {
        if trial_period == 0 {
            return Ok(0);
        }

        let total_duration = trial_period
            .checked_add(
                frequency
                    .checked_mul(max_pulls)
                    .ok_or(VelaError::Overflow)?,
            )
            .ok_or(VelaError::Overflow)?;
        let total_duration =
            i64::try_from(total_duration).map_err(|_| VelaError::Overflow)?;
        start_timestamp
            .checked_add(total_duration)
            .ok_or(VelaError::Overflow.into())
    }

    fn add_delay(start_timestamp: i64, delay: u64) -> Result<i64> {
        let delay = i64::try_from(delay).map_err(|_| VelaError::Overflow)?;
        start_timestamp
            .checked_add(delay)
            .ok_or(VelaError::Overflow.into())
    }
}

pub fn load_plan_account(plan_info: &AccountInfo<'_>) -> Result<LoadedPlanAccount> {
    require_keys_eq!(*plan_info.owner, crate::ID, VelaError::BillingTypeMismatch);

    let data = plan_info.try_borrow_data()?;
    if data.len() < VelaPlan::DISCRIMINATOR.len()
        || !data.starts_with(&VelaPlan::DISCRIMINATOR)
    {
        // Try UsagePlan discriminator
        if data.len() >= UsagePlan::DISCRIMINATOR.len()
            && data.starts_with(&UsagePlan::DISCRIMINATOR)
        {
            return load_usage_plan_inner(plan_info, &data);
        }
        return Err(VelaError::BillingTypeMismatch.into());
    }

    // Try V2 flat plan first
    {
        let mut slice: &[u8] = &data;
        if let Ok(plan) = VelaPlan::try_deserialize(&mut slice) {
            if plan.version == CURRENT_ACCOUNT_VERSION {
                validate_flat_plan(plan_info.key, &plan)?;
                return Ok(LoadedPlanAccount::Flat(plan));
            }
        }
    }

    // Try V1 flat plan
    {
        let mut legacy_slice: &[u8] = &data[VelaPlan::DISCRIMINATOR.len()..];
        if let Ok(legacy) = VelaPlanV1::deserialize(&mut legacy_slice) {
            validate_flat_plan_legacy(plan_info.key, &legacy)?;
            return Ok(LoadedPlanAccount::LegacyFlat(legacy));
        }
    }

    // Try V2 usage plan
    {
        let mut slice: &[u8] = &data;
        if let Ok(plan) = UsagePlan::try_deserialize(&mut slice) {
            if plan.version == CURRENT_ACCOUNT_VERSION {
                validate_usage_plan(plan_info.key, &plan)?;
                return Ok(LoadedPlanAccount::Usage(plan));
            }
        }
    }

    // Try V1 usage plan
    {
        let mut legacy_slice: &[u8] = &data[UsagePlan::DISCRIMINATOR.len()..];
        if let Ok(legacy) = UsagePlanV1::deserialize(&mut legacy_slice) {
            validate_usage_plan_legacy(plan_info.key, &legacy)?;
            return Ok(LoadedPlanAccount::LegacyUsage(legacy));
        }
    }

    Err(VelaError::BillingTypeMismatch.into())
}

fn load_usage_plan_inner<'info>(
    plan_info: &AccountInfo<'info>,
    data: &[u8],
) -> Result<LoadedPlanAccount> {
    // Try V2 usage plan
    {
        let mut slice: &[u8] = data;
        if let Ok(plan) = UsagePlan::try_deserialize(&mut slice) {
            if plan.version == CURRENT_ACCOUNT_VERSION {
                validate_usage_plan(plan_info.key, &plan)?;
                return Ok(LoadedPlanAccount::Usage(plan));
            }
        }
    }

    // Try V1 usage plan
    {
        let mut legacy_slice: &[u8] = &data[UsagePlan::DISCRIMINATOR.len()..];
        if let Ok(legacy) = UsagePlanV1::deserialize(&mut legacy_slice) {
            validate_usage_plan_legacy(plan_info.key, &legacy)?;
            return Ok(LoadedPlanAccount::LegacyUsage(legacy));
        }
    }

    Err(VelaError::BillingTypeMismatch.into())
}

pub fn require_plan_billing_type(
    plan: &LoadedPlanAccount,
    expected: &BillingType,
) -> Result<()> {
    require!(plan.billing_type() == *expected, VelaError::BillingTypeMismatch);
    Ok(())
}

pub fn write_plan(plan_info: &AccountInfo<'_>, plan: &VelaPlan) -> Result<()> {
    let mut data = plan_info.try_borrow_mut_data()?;
    data.fill(0);
    let mut slice: &mut [u8] = &mut data[..];
    plan.try_serialize(&mut slice)?;
    Ok(())
}

pub fn write_usage_plan(plan_info: &AccountInfo<'_>, plan: &UsagePlan) -> Result<()> {
    let mut data = plan_info.try_borrow_mut_data()?;
    data.fill(0);
    let mut slice: &mut [u8] = &mut data[..];
    plan.try_serialize(&mut slice)?;
    Ok(())
}

fn validate_flat_plan(plan_key: &Pubkey, plan: &VelaPlan) -> Result<()> {
    let plan_id_bytes = plan.plan_id.to_le_bytes();
    let (expected, _) = Pubkey::find_program_address(
        &[
            VelaPlan::SEED_PREFIX,
            plan.merchant.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(*plan_key, expected, VelaError::BillingTypeMismatch);
    Ok(())
}

fn validate_flat_plan_legacy(plan_key: &Pubkey, plan: &VelaPlanV1) -> Result<()> {
    let plan_id_bytes = plan.plan_id.to_le_bytes();
    let (expected, _) = Pubkey::find_program_address(
        &[
            VelaPlan::SEED_PREFIX,
            plan.merchant.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(*plan_key, expected, VelaError::BillingTypeMismatch);
    Ok(())
}

fn validate_usage_plan(plan_key: &Pubkey, plan: &UsagePlan) -> Result<()> {
    let plan_id_bytes = plan.plan_id.to_le_bytes();
    let (expected, _) = Pubkey::find_program_address(
        &[
            UsagePlan::SEED_PREFIX,
            plan.merchant.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(*plan_key, expected, VelaError::BillingTypeMismatch);
    Ok(())
}

fn validate_usage_plan_legacy(plan_key: &Pubkey, plan: &UsagePlanV1) -> Result<()> {
    let plan_id_bytes = plan.plan_id.to_le_bytes();
    let (expected, _) = Pubkey::find_program_address(
        &[
            UsagePlan::SEED_PREFIX,
            plan.merchant.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &crate::ID,
    );
    require_keys_eq!(*plan_key, expected, VelaError::BillingTypeMismatch);
    Ok(())
}
