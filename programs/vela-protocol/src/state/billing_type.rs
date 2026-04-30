use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Default)]
pub enum BillingType {
    #[default]
    Flat, // 0 - default, existing behavior
    Usage, // 1 - references UsagePlan
}
