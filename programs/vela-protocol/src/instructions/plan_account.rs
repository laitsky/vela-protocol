use anchor_lang::{prelude::*, AccountDeserialize};

use crate::{
    errors::VelaError,
    state::{BillingType, PlanStatus, VelaPlan, UsagePlan},
};

pub enum LoadedPlanAccount {
    Flat(VelaPlan),
    Usage(UsagePlan),
}

impl LoadedPlanAccount {
    pub fn billing_type(&self) -> BillingType {
        match self {
            Self::Flat(_) => BillingType::Flat,
            Self::Usage(_) => BillingType::Usage,
        }
    }

    pub fn merchant(&self) -> Pubkey {
        match self {
            Self::Flat(plan) => plan.merchant,
            Self::Usage(plan) => plan.merchant,
        }
    }

    pub fn credential_mint(&self) -> Pubkey {
        match self {
            Self::Flat(plan) => plan.credential_mint,
            Self::Usage(plan) => plan.credential_mint,
        }
    }

    pub fn status(&self) -> &PlanStatus {
        match self {
            Self::Flat(plan) => &plan.status,
            Self::Usage(plan) => &plan.status,
        }
    }

    pub fn mandate_amount(&self) -> u64 {
        match self {
            Self::Flat(plan) => plan.amount,
            Self::Usage(plan) => plan.max_charge_per_period,
        }
    }

    pub fn mandate_frequency(&self) -> u64 {
        match self {
            Self::Flat(plan) => plan.frequency,
            Self::Usage(plan) => plan.settlement_frequency,
        }
    }

    pub fn max_pulls(&self) -> u64 {
        match self {
            Self::Flat(plan) => plan.max_pulls,
            Self::Usage(_) => u64::MAX,
        }
    }

    pub fn plan_id(&self) -> u64 {
        match self {
            Self::Flat(plan) => plan.plan_id,
            Self::Usage(plan) => plan.plan_id,
        }
    }

    pub fn bump(&self) -> u8 {
        match self {
            Self::Flat(plan) => plan.bump,
            Self::Usage(plan) => plan.bump,
        }
    }

    pub fn expiry(&self, start_timestamp: i64) -> Result<i64> {
        match self {
            Self::Flat(plan) => {
                if plan.trial_period == 0 {
                    return Ok(0);
                }

                let total_duration = plan
                    .trial_period
                    .checked_add(
                        plan.frequency
                            .checked_mul(plan.max_pulls)
                            .ok_or(VelaError::Overflow)?,
                    )
                    .ok_or(VelaError::Overflow)?;
                let total_duration =
                    i64::try_from(total_duration).map_err(|_| VelaError::Overflow)?;
                start_timestamp
                    .checked_add(total_duration)
                    .ok_or(VelaError::Overflow.into())
            }
            Self::Usage(_) => Ok(0),
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
                let delay = i64::try_from(delay).map_err(|_| VelaError::Overflow)?;
                start_timestamp
                    .checked_add(delay)
                    .ok_or(VelaError::Overflow.into())
            }
            Self::Usage(plan) => {
                let delay =
                    i64::try_from(plan.settlement_frequency).map_err(|_| VelaError::Overflow)?;
                start_timestamp
                    .checked_add(delay)
                    .ok_or(VelaError::Overflow.into())
            }
        }
    }
}

pub fn load_plan_account(plan_info: &AccountInfo<'_>) -> Result<LoadedPlanAccount> {
    require_keys_eq!(*plan_info.owner, crate::ID, VelaError::BillingTypeMismatch);

    {
        let data = plan_info.try_borrow_data()?;
        let mut slice: &[u8] = &data;
        if let Ok(plan) = VelaPlan::try_deserialize(&mut slice) {
            validate_flat_plan(plan_info.key, &plan)?;
            return Ok(LoadedPlanAccount::Flat(plan));
        }
    }

    {
        let data = plan_info.try_borrow_data()?;
        let mut slice: &[u8] = &data;
        if let Ok(plan) = UsagePlan::try_deserialize(&mut slice) {
            validate_usage_plan(plan_info.key, &plan)?;
            return Ok(LoadedPlanAccount::Usage(plan));
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
