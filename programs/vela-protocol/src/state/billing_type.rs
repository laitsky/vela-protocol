use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum BillingType {
    Flat,  // 0 - default, existing behavior
    Usage, // 1 - references UsagePlan
}

impl Default for BillingType {
    fn default() -> Self {
        BillingType::Flat
    }
}
