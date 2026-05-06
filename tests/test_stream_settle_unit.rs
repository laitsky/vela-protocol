use anchor_lang::{error::Error, prelude::Pubkey};
use vela_protocol::errors::VelaError;
use vela_protocol::state::stream_mandate::{StreamMandate, StreamStatus};

pub use vela_protocol::ID;

mod errors {
    pub use vela_protocol::errors::*;
}

mod state {
    pub use vela_protocol::state::CURRENT_ACCOUNT_VERSION;

    pub mod stream_mandate {
        pub use vela_protocol::state::stream_mandate::*;
    }
}

#[path = "../programs/vela-protocol/src/instructions/accounts/stream_account.rs"]
#[allow(dead_code)]
mod stream_account;

use stream_account::settle_accrued_in_place;

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
        pending_nonce_short: [0u8; 8],
        _reserved_v2: [0u8; 23],
    }
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

#[test]
fn settles_active_stream_and_advances_state() {
    let mut mandate = fresh_mandate(5, 100, Some(1_000));

    let settled = settle_accrued_in_place(&mut mandate, 110).expect("settlement");

    assert_eq!(settled, 50);
    assert_eq!(mandate.total_streamed, 50);
    assert_eq!(mandate.last_settled_ts, 110);
}

#[test]
fn paused_status_noops_without_state_change() {
    let mut mandate = fresh_mandate(5, 100, Some(1_000));
    mandate.status = StreamStatus::Paused;
    mandate.paused_at = Some(100);

    let settled = settle_accrued_in_place(&mut mandate, 110).expect("paused noop");

    assert_eq!(settled, 0);
    assert_eq!(mandate.total_streamed, 0);
    assert_eq!(mandate.last_settled_ts, 100);
}

#[test]
fn cancelled_status_noops_without_state_change() {
    let mut mandate = fresh_mandate(5, 100, Some(1_000));
    mandate.status = StreamStatus::Cancelled;

    let settled = settle_accrued_in_place(&mut mandate, 110).expect("cancelled noop");

    assert_eq!(settled, 0);
    assert_eq!(mandate.total_streamed, 0);
    assert_eq!(mandate.last_settled_ts, 100);
}

#[test]
fn clock_regression_is_rejected() {
    let mut mandate = fresh_mandate(5, 100, Some(1_000));

    let error = settle_accrued_in_place(&mut mandate, 99).unwrap_err();

    assert_eq!(
        error_code(error),
        vela_error_code(VelaError::ClockRegression)
    );
    assert_eq!(mandate.total_streamed, 0);
    assert_eq!(mandate.last_settled_ts, 100);
}

#[test]
fn i64_max_elapsed_at_unit_rate_stays_lossless() {
    let mut mandate = fresh_mandate(1, 0, None);

    let settled = settle_accrued_in_place(&mut mandate, i64::MAX).expect("wide settle");

    let expected = u64::try_from(i64::MAX).expect("i64::MAX fits in u64");

    assert_eq!(settled, expected);
    assert_eq!(mandate.total_streamed, expected);
    assert_eq!(mandate.last_settled_ts, i64::MAX);
}

#[test]
fn gross_over_u64_max_without_cap_errors() {
    let mut mandate = fresh_mandate(3, 0, None);

    let error = settle_accrued_in_place(&mut mandate, i64::MAX).unwrap_err();

    assert_eq!(error_code(error), vela_error_code(VelaError::Overflow));
    assert_eq!(mandate.total_streamed, 0);
    assert_eq!(mandate.last_settled_ts, 0);
}

#[test]
fn cap_clamps_settlement_to_remaining_amount() {
    let mut mandate = fresh_mandate(10, 100, Some(50));

    let settled = settle_accrued_in_place(&mut mandate, 200).expect("cap clamp");

    assert_eq!(settled, 50);
    assert_eq!(mandate.total_streamed, 50);
    assert_eq!(mandate.last_settled_ts, 200);
}

#[test]
fn none_cap_returns_full_gross() {
    let mut mandate = fresh_mandate(7, 100, None);

    let settled = settle_accrued_in_place(&mut mandate, 103).expect("uncapped settle");

    assert_eq!(settled, 21);
    assert_eq!(mandate.total_streamed, 21);
    assert_eq!(mandate.last_settled_ts, 103);
}

#[test]
fn non_positive_settlements_do_not_change_timestamp() {
    let mut mandate = fresh_mandate(5, 100, Some(1_000));
    mandate.status = StreamStatus::Paused;
    mandate.paused_at = Some(100);

    let settled = settle_accrued_in_place(&mut mandate, 140).expect("paused noop");

    assert_eq!(settled, 0);
    assert_eq!(mandate.last_settled_ts, 100);
}
