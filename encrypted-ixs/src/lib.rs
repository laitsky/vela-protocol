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

    /// Simple per-unit usage pricing: charge = usage_units * rate_per_unit, capped at max_charge.
    /// Returns plaintext u64 approved_amount written to PullApproval by usage_computation_callback.
    #[instruction]
    pub fn usage_charge(
        usage_units: Enc<Shared, u64>,
        rate_per_unit: Enc<Shared, u64>,
        max_charge: Enc<Shared, u64>,
    ) -> u64 {
        let units = usage_units.to_arcis();
        let rate = rate_per_unit.to_arcis();
        let cap = max_charge.to_arcis();

        let charge = units * rate;
        // Cap at max_charge -- both branches execute (MPC constraint, Pitfall 3)
        let capped = if charge > cap { cap } else { charge };

        capped.reveal()
    }

    /// Multi-tier usage pricing: evaluates up to 5 tiers on ciphertext with a fixed-size loop.
    /// Fixed-size loop (always 5 iterations) satisfies MPC deterministic branching requirement.
    /// Returns plaintext u64 approved_amount written to PullApproval by usage_computation_callback.
    #[instruction]
    pub fn tiered_pricing(
        usage_units: Enc<Shared, u64>,
        tier_boundaries: Enc<Shared, [u64; 5]>,
        tier_rates: Enc<Shared, [u64; 5]>,
        tier_count: Enc<Shared, u8>,
        max_charge: Enc<Shared, u64>,
    ) -> u64 {
        let units = usage_units.to_arcis();
        let boundaries = tier_boundaries.to_arcis();
        let rates = tier_rates.to_arcis();
        let count = tier_count.to_arcis();
        let cap = max_charge.to_arcis();

        // Fixed-size loop -- MPC requires deterministic branching
        // All 5 iterations execute regardless of tier_count
        // Use explicit per-tier active flags to avoid implicit u8->u64 conversion in arcis
        let tier0_active = 0u8 < count;
        let tier1_active = 1u8 < count;
        let tier2_active = 2u8 < count;
        let tier3_active = 3u8 < count;
        let tier4_active = 4u8 < count;

        let mut total_charge: u64 = 0;
        let mut remaining_units = units;

        // Tier 0
        let units_in_tier0 = if remaining_units > boundaries[0] {
            boundaries[0]
        } else {
            remaining_units
        };
        let tier_charge0 = if tier0_active {
            units_in_tier0 * rates[0]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge0;
        remaining_units = if remaining_units > boundaries[0] {
            remaining_units - boundaries[0]
        } else {
            0u64
        };

        // Tier 1
        let units_in_tier1 = if remaining_units > boundaries[1] {
            boundaries[1]
        } else {
            remaining_units
        };
        let tier_charge1 = if tier1_active {
            units_in_tier1 * rates[1]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge1;
        remaining_units = if remaining_units > boundaries[1] {
            remaining_units - boundaries[1]
        } else {
            0u64
        };

        // Tier 2
        let units_in_tier2 = if remaining_units > boundaries[2] {
            boundaries[2]
        } else {
            remaining_units
        };
        let tier_charge2 = if tier2_active {
            units_in_tier2 * rates[2]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge2;
        remaining_units = if remaining_units > boundaries[2] {
            remaining_units - boundaries[2]
        } else {
            0u64
        };

        // Tier 3
        let units_in_tier3 = if remaining_units > boundaries[3] {
            boundaries[3]
        } else {
            remaining_units
        };
        let tier_charge3 = if tier3_active {
            units_in_tier3 * rates[3]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge3;
        remaining_units = if remaining_units > boundaries[3] {
            remaining_units - boundaries[3]
        } else {
            0u64
        };

        // Tier 4
        let units_in_tier4 = if remaining_units > boundaries[4] {
            boundaries[4]
        } else {
            remaining_units
        };
        let tier_charge4 = if tier4_active {
            units_in_tier4 * rates[4]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge4;

        let capped = if total_charge > cap {
            cap
        } else {
            total_charge
        };
        capped.reveal()
    }
}
