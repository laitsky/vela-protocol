use std::convert::TryFrom;

use anchor_lang::{prelude::*, Discriminator};

use crate::{
    errors::VelaError,
    state::{
        stream_mandate::{StreamMandate, StreamStatus},
        CURRENT_ACCOUNT_VERSION,
    },
};

pub fn load_stream_mandate(info: &AccountInfo<'_>) -> Result<StreamMandate> {
    if *info.owner != crate::ID {
        return Err(ProgramError::IncorrectProgramId.into());
    }

    let data = info.try_borrow_data()?;
    if data.len() < StreamMandate::DISCRIMINATOR.len() {
        return Err(ProgramError::InvalidAccountData.into());
    }
    if !data.starts_with(StreamMandate::DISCRIMINATOR) {
        return Err(VelaError::WrongAccountType.into());
    }

    let mut slice: &[u8] = &data;
    let mandate =
        StreamMandate::try_deserialize(&mut slice).map_err(|_| ProgramError::InvalidAccountData)?;
    if mandate.version != CURRENT_ACCOUNT_VERSION {
        return Err(ProgramError::InvalidAccountData.into());
    }

    Ok(mandate)
}

pub fn validate_stream_mandate_address(key: &Pubkey, mandate: &StreamMandate) -> Result<()> {
    let expected = Pubkey::find_program_address(
        &[
            StreamMandate::SEED_PREFIX,
            mandate.subscriber.as_ref(),
            mandate.merchant.as_ref(),
            mandate.mandate_index.to_le_bytes().as_ref(),
        ],
        &crate::ID,
    )
    .0;

    if *key != expected {
        return Err(VelaError::InvalidStreamAddress.into());
    }

    Ok(())
}

pub fn write_stream_mandate(info: &AccountInfo<'_>, mandate: &StreamMandate) -> Result<()> {
    let mut data = info.try_borrow_mut_data()?;
    data.fill(0);

    let mut slice: &mut [u8] = &mut data[..];
    mandate.try_serialize(&mut slice)?;
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn settle_accrued_in_place(mandate: &mut StreamMandate, clock_now: i64) -> Result<u64> {
    if mandate.status != StreamStatus::Active {
        return Ok(0);
    }
    if mandate.paused_at.is_some() {
        return Ok(0);
    }

    let elapsed_i64 = clock_now
        .checked_sub(mandate.last_settled_ts)
        .ok_or(VelaError::ClockRegression)?;
    require!(elapsed_i64 >= 0, VelaError::ClockRegression);

    let elapsed = u128::from(u64::try_from(elapsed_i64).map_err(|_| VelaError::Overflow)?);
    let gross = elapsed
        .checked_mul(u128::from(mandate.rate_per_second))
        .ok_or(VelaError::Overflow)?;
    let remaining = match mandate.max_streamed {
        Some(cap) => u128::from(cap)
            .checked_sub(u128::from(mandate.total_streamed))
            .ok_or(VelaError::StreamCapExceeded)?,
        None => u128::MAX,
    };
    let amount_u128 = core::cmp::min(gross, remaining);
    let amount = u64::try_from(amount_u128).map_err(|_| VelaError::Overflow)?;

    mandate.total_streamed = mandate
        .total_streamed
        .checked_add(amount)
        .ok_or(VelaError::Overflow)?;
    mandate.last_settled_ts = clock_now;

    Ok(amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_mandate(rate: u64, last: i64, cap: Option<u64>) -> StreamMandate {
        StreamMandate {
            version: 1,
            subscriber: Pubkey::new_unique(),
            merchant: Pubkey::new_unique(),
            mint: Pubkey::new_unique(),
            rate_per_second: rate,
            authorized_max_rate: rate,
            last_settled_ts: last,
            total_streamed: 0,
            max_streamed: cap,
            paused_at: None,
            min_settle_interval: 60,
            status: StreamStatus::Active,
            mandate_index: 0,
            bump: 255,
            pending_new_rate_per_second: 0,
            pending_new_authorized_max_rate: 0,
            pending_effective_at: 0,
            pending_change_type: 0,
            pending_nonce_short: [0; 8],
            _reserved_v2: [0u8; 23],
        }
    }

    #[test]
    fn settle_helper_advances_active_mandate() {
        let mut mandate = fresh_mandate(5, 100, Some(1_000));

        let settled = settle_accrued_in_place(&mut mandate, 110).expect("settlement");

        assert_eq!(settled, 50);
        assert_eq!(mandate.total_streamed, 50);
        assert_eq!(mandate.last_settled_ts, 110);
    }

    #[test]
    fn settle_helper_noops_when_paused() {
        let mut mandate = fresh_mandate(5, 100, Some(1_000));
        mandate.status = StreamStatus::Paused;
        mandate.paused_at = Some(100);

        let settled = settle_accrued_in_place(&mut mandate, 110).expect("paused noop");

        assert_eq!(settled, 0);
        assert_eq!(mandate.total_streamed, 0);
        assert_eq!(mandate.last_settled_ts, 100);
    }
}
