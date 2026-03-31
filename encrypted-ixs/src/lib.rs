use arcis::*;

#[encrypted]
mod circuits {
    use arcis::*;

    pub struct MandateValidationInput {
        mandate_amount: u64,
        plan_amount: u64,
        subscriber_balance: u64,
        current_timestamp: i64,
        next_payment_due: i64,
        expiry: i64,
        pulls_executed: u64,
        max_pulls: u64,
    }

    #[instruction]
    pub fn validate_mandate(input: Enc<Shared, MandateValidationInput>) -> bool {
        let v = input.to_arcis();

        // All comparisons run on encrypted data
        // Comparisons are expensive in MPC -- compute once, reuse
        let amount_valid = v.mandate_amount == v.plan_amount;
        let balance_sufficient = v.subscriber_balance >= v.plan_amount;
        let timing_valid = v.current_timestamp >= v.next_payment_due;

        // Both branches always execute (MPC constraint -- Pitfall 5)
        // Use conditional assignment instead of if/else with early return
        let not_expired = if v.expiry == 0i64 {
            true
        } else {
            v.current_timestamp < v.expiry
        };

        let pulls_remaining = v.pulls_executed < v.max_pulls;

        let approved =
            amount_valid & balance_sufficient & timing_valid & not_expired & pulls_remaining;

        approved.reveal()
    }

    #[instruction]
    pub fn record_billing_event(
        amount_charged: u64,
        timestamp: i64,
        pulls_executed: u64,
        billing_period_start: i64,
        billing_period_end: i64,
        payment_method: u64,
    ) -> Enc<Mxe, [u64; 8]> {
        let packed: [u64; 8] = [
            amount_charged,
            timestamp as u64,
            pulls_executed,
            billing_period_start as u64,
            billing_period_end as u64,
            payment_method,
            0u64,
            0u64,
        ];

        Mxe::get().from_arcis(packed)
    }
}
