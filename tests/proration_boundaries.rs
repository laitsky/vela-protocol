mod errors {
    pub use vela_protocol::errors::*;
}

#[path = "../programs/vela-protocol/src/instructions/proration.rs"]
#[allow(dead_code)]
mod proration;

use proration::compute_proration;

const DAY_SECONDS: u64 = 86_400;
const DST_BOUNDARY_TS: i64 = 1_805_157_600;
const LEAP_BOUNDARY_TS: i64 = 1_798_761_600;

fn elapsed_seconds(now: i64, last: i64) -> u64 {
    u64::try_from(now.checked_sub(last).expect("timestamp subtraction"))
        .expect("elapsed time must be non-negative")
}

#[test]
fn period_start_charges_full_new_amount_for_fresh_period() {
    let amount = compute_proration(0, 10_000_000, 0, 30 * DAY_SECONDS).expect("period start");

    assert_eq!(amount, 10_000_000);
}

#[test]
fn period_end_leaves_no_remaining_charge() {
    let amount =
        compute_proration(0, 10_000_000, 30 * DAY_SECONDS, 30 * DAY_SECONDS).expect("period end");

    assert_eq!(amount, 0);
}

#[test]
fn partial_second_truncates_toward_subscriber() {
    let amount = compute_proration(30, 60, 1, 60).expect("partial second");

    assert_eq!(amount, 29);
}

#[test]
fn dst_boundary_uses_plain_unix_elapsed_seconds() {
    let via_boundary = compute_proration(
        10_000_000,
        20_000_000,
        elapsed_seconds(DST_BOUNDARY_TS, DST_BOUNDARY_TS - 3_600),
        DAY_SECONDS,
    )
    .expect("dst boundary");
    let via_plain_elapsed =
        compute_proration(10_000_000, 20_000_000, 3_600, DAY_SECONDS).expect("plain elapsed");

    assert_eq!(via_boundary, via_plain_elapsed);
}

#[test]
fn repeated_leap_second_is_zero_elapsed_noop() {
    let elapsed = elapsed_seconds(LEAP_BOUNDARY_TS, LEAP_BOUNDARY_TS);
    let amount = compute_proration(10_000_000, 10_000_000, elapsed, 60).expect("leap second");

    assert_eq!(elapsed, 0);
    assert_eq!(amount, 0);
}
