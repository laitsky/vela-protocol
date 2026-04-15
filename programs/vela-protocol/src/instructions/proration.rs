use anchor_lang::prelude::*;
use spl_math::precise_number::PreciseNumber;

use crate::errors::VelaError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProrationOutcome {
    NoOp,
    ChargeNow,
    CreditNow,
    ScheduledChange,
}

#[allow(dead_code)]
fn precise(value: u128) -> Result<PreciseNumber> {
    PreciseNumber::new(value).ok_or_else(|| error!(VelaError::MathOverflow))
}

#[allow(dead_code)]
fn checked_mul(lhs: &PreciseNumber, rhs: &PreciseNumber) -> Result<PreciseNumber> {
    lhs.checked_mul(rhs)
        .ok_or_else(|| error!(VelaError::MathOverflow))
}

#[allow(dead_code)]
fn checked_div(lhs: &PreciseNumber, rhs: &PreciseNumber) -> Result<PreciseNumber> {
    lhs.checked_div(rhs)
        .ok_or_else(|| error!(VelaError::MathOverflow))
}

#[allow(dead_code)]
fn checked_sub(
    lhs: &PreciseNumber,
    rhs: &PreciseNumber,
    error_code: VelaError,
) -> Result<PreciseNumber> {
    lhs.checked_sub(rhs).ok_or_else(|| error!(error_code))
}

#[allow(dead_code)]
fn floor_to_u128(value: &PreciseNumber) -> Result<u128> {
    value
        .floor()
        .ok_or_else(|| error!(VelaError::MathOverflow))?
        .to_imprecise()
        .ok_or_else(|| error!(VelaError::MathOverflow))
}

#[allow(dead_code)]
pub(crate) fn compute_proration(
    plan_amount_old: u64,
    plan_amount_new: u64,
    elapsed_seconds: u64,
    period_total_seconds: u64,
) -> Result<i128> {
    require!(period_total_seconds > 0, VelaError::MathDivByZero);
    require!(elapsed_seconds <= period_total_seconds, VelaError::InvalidElapsed);

    let old = precise(plan_amount_old as u128)?;
    let new_amount = precise(plan_amount_new as u128)?;
    let elapsed = precise(elapsed_seconds as u128)?;
    let total = precise(period_total_seconds as u128)?;
    let remaining = checked_sub(&total, &elapsed, VelaError::MathUnderflow)?;

    let used_old = checked_div(&checked_mul(&old, &elapsed)?, &total)?;
    let unused_old = checked_sub(&old, &used_old, VelaError::MathUnderflow)?;
    let new_for_remaining = checked_div(&checked_mul(&new_amount, &remaining)?, &total)?;

    let (signed_delta, is_charge) = if new_for_remaining.greater_than_or_equal(&unused_old) {
        (
            checked_sub(&new_for_remaining, &unused_old, VelaError::MathUnderflow)?,
            true,
        )
    } else {
        (
            checked_sub(&unused_old, &new_for_remaining, VelaError::MathUnderflow)?,
            false,
        )
    };

    let raw = floor_to_u128(&signed_delta)?;
    let signed = i128::try_from(raw).map_err(|_| error!(VelaError::MathOverflow))?;

    Ok(if is_charge { signed } else { -signed })
}

#[cfg(test)]
mod tests {
    use anchor_lang::error;

    use super::{compute_proration, ProrationOutcome};
    use crate::errors::VelaError;

    #[test]
    fn exact_half_period_upgrade_charges_expected_amount() {
        let amount = compute_proration(10_000_000, 20_000_000, 15 * 86_400, 30 * 86_400)
            .expect("charge proration");

        assert_eq!(amount, 5_000_000);
        assert_eq!(classify(amount), ProrationOutcome::ChargeNow);
    }

    #[test]
    fn exact_half_period_downgrade_credits_expected_amount() {
        let amount = compute_proration(20_000_000, 10_000_000, 15 * 86_400, 30 * 86_400)
            .expect("credit proration");

        assert_eq!(amount, -5_000_000);
        assert_eq!(classify(amount), ProrationOutcome::CreditNow);
    }

    #[test]
    fn charge_path_truncates_subcent_fraction() {
        let amount = compute_proration(10_000_001, 20_000_000, 15 * 86_400, 30 * 86_400)
            .expect("sub-cent charge");

        assert_eq!(amount, 4_999_999);
    }

    #[test]
    fn credit_path_truncates_subcent_fraction() {
        let amount = compute_proration(20_000_000, 10_000_001, 15 * 86_400, 30 * 86_400)
            .expect("sub-cent credit");

        assert_eq!(amount, -4_999_999);
    }

    #[test]
    fn same_plan_amount_is_noop() {
        let amount = compute_proration(10_000_000, 10_000_000, 7 * 86_400, 30 * 86_400)
            .expect("noop proration");

        assert_eq!(amount, 0);
        assert_eq!(classify(amount), ProrationOutcome::NoOp);
    }

    #[test]
    fn zero_period_is_rejected() {
        let error = compute_proration(10_000_000, 20_000_000, 1, 0).unwrap_err();

        assert_eq!(error, error!(VelaError::MathDivByZero));
    }

    #[test]
    fn elapsed_beyond_period_is_rejected() {
        let error = compute_proration(10_000_000, 20_000_000, 31, 30).unwrap_err();

        assert_eq!(error, error!(VelaError::InvalidElapsed));
    }

    fn classify(amount: i128) -> ProrationOutcome {
        match amount.cmp(&0) {
            core::cmp::Ordering::Greater => ProrationOutcome::ChargeNow,
            core::cmp::Ordering::Less => ProrationOutcome::CreditNow,
            core::cmp::Ordering::Equal => ProrationOutcome::NoOp,
        }
    }
}
