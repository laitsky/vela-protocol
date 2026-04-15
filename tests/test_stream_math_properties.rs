use anchor_lang::{error::Error, prelude::Pubkey};
use proptest::prelude::*;
use vela_protocol::errors::VelaError;
use vela_protocol::instructions::stream_account::settle_accrued_in_place;
use vela_protocol::state::stream_mandate::{StreamMandate, StreamStatus};

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
        _reserved: [0u8; 56],
    }
}

fn distribute(total: u64, splits: u64) -> Vec<u64> {
    let base = total / splits;
    let remainder = total % splits;
    (0..splits)
        .map(|i| if i == splits - 1 { base + remainder } else { base })
        .collect()
}

fn error_code(error: Error) -> u32 {
    match error {
        Error::AnchorError(anchor_error) => anchor_error.error_code_number,
        other => panic!("expected AnchorError, got {other:?}"),
    }
}

fn vela_error_code(error: VelaError) -> u32 {
    match Error::from(error) {
        Error::AnchorError(anchor_error) => anchor_error.error_code_number,
        other => panic!("expected AnchorError, got {other:?}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn settlement_frequency_invariance_no_cap(
        rate in 1u64..=1_000_000_000u64,
        total_secs in 60u64..=86_400u64,
        n_splits in 2u64..=60u64,
    ) {
        let mut single = fresh_mandate(rate, 0, None);
        let single_total = settle_accrued_in_place(&mut single, total_secs as i64).unwrap();

        let mut multi = fresh_mandate(rate, 0, None);
        let mut multi_total = 0u64;
        let mut clock = 0i64;
        for interval in distribute(total_secs, n_splits) {
            clock += interval as i64;
            multi_total = multi_total
                .checked_add(settle_accrued_in_place(&mut multi, clock).unwrap())
                .unwrap();
        }

        prop_assert_eq!(single_total, multi_total);
        prop_assert_eq!(single.total_streamed, multi.total_streamed);
    }

    #[test]
    fn settlement_frequency_invariance_with_cap(
        rate in 1u64..=1_000_000u64,
        total_secs in 60u64..=86_400u64,
        n_splits in 2u64..=24u64,
        cap in 1u64..=(u32::MAX as u64),
    ) {
        let mut single = fresh_mandate(rate, 0, Some(cap));
        let single_total = settle_accrued_in_place(&mut single, total_secs as i64).unwrap();

        let mut multi = fresh_mandate(rate, 0, Some(cap));
        let mut multi_total = 0u64;
        let mut clock = 0i64;
        for interval in distribute(total_secs, n_splits) {
            clock += interval as i64;
            multi_total = multi_total
                .checked_add(settle_accrued_in_place(&mut multi, clock).unwrap())
                .unwrap();
        }

        let unclamped = (rate as u128) * (total_secs as u128);
        let expected = core::cmp::min(unclamped, cap as u128);

        prop_assert_eq!(single_total as u128, expected);
        prop_assert_eq!(multi_total as u128, expected);
        prop_assert!(multi.total_streamed <= cap);
    }

    #[test]
    fn u64_max_boundary(
        rate in (u64::MAX - 100)..=u64::MAX,
        elapsed in (u64::MAX - 100)..=u64::MAX,
    ) {
        let clock = (elapsed as u128).min(i64::MAX as u128) as i64;
        let mut mandate = fresh_mandate(rate, 0, None);

        match settle_accrued_in_place(&mut mandate, clock) {
            Ok(amount) => {
                let expected = (clock as u128).checked_mul(rate as u128);
                match expected {
                    Some(value) if value <= u64::MAX as u128 => prop_assert_eq!(amount as u128, value),
                    _ => prop_assert!(false, "settlement succeeded despite overflowing u64 math"),
                }
            }
            Err(error) => {
                prop_assert_eq!(error_code(error), vela_error_code(VelaError::Overflow));
            }
        }
    }

    #[test]
    fn settle_then_mutate_invariant(
        rate in 1u64..=1_000_000u64,
        pause_ts in 60i64..=86_400i64,
        resume_ts in 86_401i64..=172_800i64,
        cancel_ts in 172_801i64..=259_200i64,
        update_ts in 259_201i64..=345_600i64,
        new_rate in 1u64..=1_000_000u64,
    ) {
        let create = fresh_mandate(rate, 0, None);
        prop_assert_eq!(create.last_settled_ts, 0);
        prop_assert_eq!(create.total_streamed, 0);

        let mut mandate = fresh_mandate(rate, 0, None);

        let pause_before_total = mandate.total_streamed;
        let pause_amount = settle_accrued_in_place(&mut mandate, pause_ts).unwrap();
        prop_assert!(mandate.total_streamed >= pause_before_total);
        prop_assert_eq!(mandate.last_settled_ts, pause_ts);
        prop_assert_eq!(pause_amount, mandate.total_streamed);
        mandate.paused_at = Some(pause_ts);
        mandate.status = StreamStatus::Paused;

        let resume_before_total = mandate.total_streamed;
        mandate.last_settled_ts = resume_ts;
        mandate.paused_at = None;
        mandate.status = StreamStatus::Active;
        prop_assert_eq!(mandate.total_streamed, resume_before_total);
        prop_assert_eq!(mandate.last_settled_ts, resume_ts);

        let cancel_before_total = mandate.total_streamed;
        let cancel_amount = settle_accrued_in_place(&mut mandate, cancel_ts).unwrap();
        prop_assert!(mandate.total_streamed >= cancel_before_total);
        prop_assert_eq!(mandate.last_settled_ts, cancel_ts);
        prop_assert_eq!(mandate.total_streamed, cancel_before_total + cancel_amount);
        mandate.status = StreamStatus::Cancelled;
        mandate.paused_at = None;

        mandate.status = StreamStatus::Active;
        let update_before_total = mandate.total_streamed;
        let update_amount = settle_accrued_in_place(&mut mandate, update_ts).unwrap();
        prop_assert!(mandate.total_streamed >= update_before_total);
        prop_assert_eq!(mandate.last_settled_ts, update_ts);
        prop_assert_eq!(mandate.total_streamed, update_before_total + update_amount);
        mandate.rate_per_second = new_rate;

        let mut settle_only = fresh_mandate(rate, 0, None);
        let settle_amount = settle_accrued_in_place(&mut settle_only, pause_ts).unwrap();
        prop_assert_eq!(settle_only.last_settled_ts, pause_ts);
        prop_assert_eq!(settle_only.total_streamed, settle_amount);
    }
}
