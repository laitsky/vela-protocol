use anchor_lang::prelude::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::stream_mandate::{StreamMandate, StreamStatus};

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
