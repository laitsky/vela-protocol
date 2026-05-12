#![allow(clippy::assign_op_pattern, clippy::implicit_saturating_sub)]

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
    pub fn usage_charge(usage_units: Enc<Shared, u64>, rate_per_unit: u64, max_charge: u64) -> u64 {
        let units = usage_units.to_arcis();

        let charge = units * rate_per_unit;
        // Cap at max_charge -- both branches execute (MPC constraint, Pitfall 3)
        let capped = if charge > max_charge {
            max_charge
        } else {
            charge
        };

        capped.reveal()
    }

    /// Multi-tier usage pricing: evaluates up to 5 tiers on ciphertext with a fixed-size loop.
    /// Fixed-size loop (always 5 iterations) satisfies MPC deterministic branching requirement.
    /// Returns plaintext u64 approved_amount written to PullApproval by usage_computation_callback.
    #[allow(clippy::too_many_arguments)]
    #[instruction]
    pub fn tiered_pricing(
        usage_units: Enc<Shared, u64>,
        tier0_up_to: u64,
        tier1_up_to: u64,
        tier2_up_to: u64,
        tier3_up_to: u64,
        tier4_up_to: u64,
        tier0_rate: u64,
        tier1_rate: u64,
        tier2_rate: u64,
        tier3_rate: u64,
        tier4_rate: u64,
        tier_count: u8,
        max_charge: u64,
    ) -> u64 {
        let units = usage_units.to_arcis();
        let boundaries = [
            tier0_up_to,
            tier1_up_to,
            tier2_up_to,
            tier3_up_to,
            tier4_up_to,
        ];
        let rates = [tier0_rate, tier1_rate, tier2_rate, tier3_rate, tier4_rate];

        // Fixed-size loop -- MPC requires deterministic branching
        // All 5 iterations execute regardless of tier_count
        // Use explicit per-tier active flags to avoid implicit u8->u64 conversion in arcis
        let tier0_active = 0u8 < tier_count;
        let tier1_active = 1u8 < tier_count;
        let tier2_active = 2u8 < tier_count;
        let tier3_active = 3u8 < tier_count;
        let tier4_active = 4u8 < tier_count;

        let mut total_charge: u64 = 0;

        // Tier 0. A single active tier may use boundary 0 as unlimited.
        let tier0_unlimited = tier0_active & (boundaries[0] == 0u64);
        let units_in_tier0 = if tier0_unlimited {
            units
        } else if units > boundaries[0] {
            boundaries[0]
        } else {
            units
        };
        let tier_charge0 = if tier0_active {
            units_in_tier0 * rates[0]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge0;

        // Tier 1
        let tier1_unlimited = tier1_active & (boundaries[1] == 0u64);
        let tier1_upper = if tier1_unlimited {
            units
        } else if units > boundaries[1] {
            boundaries[1]
        } else {
            units
        };
        let units_in_tier1 = if tier1_upper > boundaries[0] {
            tier1_upper - boundaries[0]
        } else {
            0u64
        };
        let tier_charge1 = if tier1_active {
            units_in_tier1 * rates[1]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge1;

        // Tier 2
        let tier2_unlimited = tier2_active & (boundaries[2] == 0u64);
        let tier2_upper = if tier2_unlimited {
            units
        } else if units > boundaries[2] {
            boundaries[2]
        } else {
            units
        };
        let units_in_tier2 = if tier2_upper > boundaries[1] {
            tier2_upper - boundaries[1]
        } else {
            0u64
        };
        let tier_charge2 = if tier2_active {
            units_in_tier2 * rates[2]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge2;

        // Tier 3
        let tier3_unlimited = tier3_active & (boundaries[3] == 0u64);
        let tier3_upper = if tier3_unlimited {
            units
        } else if units > boundaries[3] {
            boundaries[3]
        } else {
            units
        };
        let units_in_tier3 = if tier3_upper > boundaries[2] {
            tier3_upper - boundaries[2]
        } else {
            0u64
        };
        let tier_charge3 = if tier3_active {
            units_in_tier3 * rates[3]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge3;

        // Tier 4
        let tier4_unlimited = tier4_active & (boundaries[4] == 0u64);
        let tier4_upper = if tier4_unlimited {
            units
        } else if units > boundaries[4] {
            boundaries[4]
        } else {
            units
        };
        let units_in_tier4 = if tier4_upper > boundaries[3] {
            tier4_upper - boundaries[3]
        } else {
            0u64
        };
        let tier_charge4 = if tier4_active {
            units_in_tier4 * rates[4]
        } else {
            0u64
        };
        total_charge = total_charge + tier_charge4;

        let capped = if total_charge > max_charge {
            max_charge
        } else {
            total_charge
        };
        capped.reveal()
    }
}

pub fn tiered_pricing_plaintext_model(
    usage_units: u64,
    tier_boundaries: [u64; 5],
    tier_rates: [u64; 5],
    tier_count: u8,
    max_charge: u64,
) -> u64 {
    let mut total_charge = 0u64;
    let mut previous_bound = 0u64;

    for i in 0..usize::from(tier_count).min(5) {
        let boundary = tier_boundaries[i];
        let upper = if boundary == 0 {
            usage_units
        } else {
            usage_units.min(boundary)
        };
        let units_in_tier = upper.saturating_sub(previous_bound);
        total_charge = total_charge.saturating_add(units_in_tier.saturating_mul(tier_rates[i]));
        if boundary != 0 {
            previous_bound = boundary;
        }
    }

    total_charge.min(max_charge)
}

#[cfg(test)]
mod tests {
    use super::tiered_pricing_plaintext_model;
    use proptest::prelude::*;

    fn reference_cumulative_pricing(
        usage_units: u64,
        boundaries: [u64; 5],
        rates: [u64; 5],
        tier_count: u8,
        cap: u64,
    ) -> u64 {
        let mut total = 0u64;
        let mut previous = 0u64;
        for index in 0..usize::from(tier_count).min(5) {
            let boundary = boundaries[index];
            let upper = if boundary == 0 {
                usage_units
            } else {
                usage_units.min(boundary)
            };
            if upper > previous {
                total = total.saturating_add((upper - previous).saturating_mul(rates[index]));
            }
            if boundary != 0 {
                previous = boundary;
            }
        }
        total.min(cap)
    }

    #[test]
    fn tiered_pricing_matches_plaintext_model_two_tier_unlimited() {
        let boundaries = [1_000, 0, 0, 0, 0];
        let rates = [100, 50, 0, 0, 0];

        assert_eq!(
            tiered_pricing_plaintext_model(750, boundaries, rates, 2, u64::MAX),
            75_000
        );
        assert_eq!(
            tiered_pricing_plaintext_model(1_500, boundaries, rates, 2, u64::MAX),
            125_000
        );
    }

    #[test]
    fn tiered_pricing_matches_plaintext_model_three_tier_cumulative() {
        let boundaries = [100, 500, 0, 0, 0];
        let rates = [10, 8, 5, 0, 0];

        assert_eq!(
            tiered_pricing_plaintext_model(50, boundaries, rates, 3, u64::MAX),
            500
        );
        assert_eq!(
            tiered_pricing_plaintext_model(300, boundaries, rates, 3, u64::MAX),
            2_600
        );
        assert_eq!(
            tiered_pricing_plaintext_model(700, boundaries, rates, 3, u64::MAX),
            5_200
        );
    }

    #[test]
    fn tiered_pricing_caps_charge() {
        let boundaries = [100, 0, 0, 0, 0];
        let rates = [10, 10, 0, 0, 0];

        assert_eq!(
            tiered_pricing_plaintext_model(1_000, boundaries, rates, 2, 1_234),
            1_234
        );
    }

    #[test]
    fn tiered_pricing_handles_exact_boundaries_and_unused_tiers() {
        let boundaries = [100, 500, 1_000, 0, 0];
        let rates = [10, 8, 6, 4, 999];

        assert_eq!(
            tiered_pricing_plaintext_model(100, boundaries, rates, 4, u64::MAX),
            1_000
        );
        assert_eq!(
            tiered_pricing_plaintext_model(500, boundaries, rates, 4, u64::MAX),
            4_200
        );
        assert_eq!(
            tiered_pricing_plaintext_model(1_000, boundaries, rates, 4, u64::MAX),
            7_200
        );
        assert_eq!(
            tiered_pricing_plaintext_model(1_250, boundaries, rates, 4, u64::MAX),
            8_200
        );
    }

    proptest! {
        #[test]
        fn tiered_pricing_matches_reference_model_for_valid_tiers(
            usage_units in 0u64..1_000_000,
            first in 1u64..10_000,
            second_delta in 1u64..10_000,
            rate0 in 0u64..1_000,
            rate1 in 0u64..1_000,
            rate2 in 0u64..1_000,
            cap in 0u64..10_000_000,
        ) {
            let second = first + second_delta;
            let boundaries = [first, second, 0, 0, 0];
            let rates = [rate0, rate1, rate2, 0, 0];
            let charge = tiered_pricing_plaintext_model(usage_units, boundaries, rates, 3, cap);
            let reference = reference_cumulative_pricing(usage_units, boundaries, rates, 3, cap);
            prop_assert_eq!(charge, reference);
            prop_assert!(charge <= cap);
        }
    }
}
