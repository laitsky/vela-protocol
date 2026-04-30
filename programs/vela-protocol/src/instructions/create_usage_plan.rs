use anchor_lang::prelude::*;
use solana_pubkey::Pubkey as SplPubkey;

use crate::errors::VelaError;
use crate::instructions::merchant_account::{
    ensure_merchant_state, resolve_merchant_credential_mint,
};
use crate::state::{
    PlanStatus, PricingTier, UsagePlan, ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION,
};

#[derive(Accounts)]
#[instruction(plan_id: u64)]
pub struct CreateUsagePlan<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(mut, seeds = [crate::state::MerchantState::SEED_PREFIX, merchant.key().as_ref()], bump)]
    pub merchant_state: UncheckedAccount<'info>,

    #[account(
        init,
        payer = merchant,
        space = UsagePlan::SIZE,
        seeds = [
            UsagePlan::SEED_PREFIX,
            merchant.key().as_ref(),
            plan_id.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub usage_plan: Account<'info, UsagePlan>,

    #[account(
        mut,
        seeds = [
            b"usage_credential",
            merchant.key().as_ref(),
            plan_id.to_le_bytes().as_ref()
        ],
        bump
    )]
    /// CHECK: PDA mint account is created via manual CPI and constrained by seeds.
    pub credential_mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: Address constraint ensures this is the Token-2022 program.
    pub token_2022_program: UncheckedAccount<'info>,

    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<CreateUsagePlan>,
    plan_id: u64,
    unit_name: [u8; 32],
    tiers: Vec<PricingTier>,
    max_charge_per_period: u64,
    settlement_frequency: u64,
) -> Result<()> {
    // Validate tier count: must be 1-5
    require!(
        !tiers.is_empty() && tiers.len() <= 5,
        VelaError::InvalidTierCount
    );

    // Validate max charge and settlement frequency
    require!(max_charge_per_period > 0, VelaError::InvalidAmount);
    require!(settlement_frequency > 0, VelaError::InvalidFrequency);

    // Validate tier boundaries are monotonically increasing (except last which can be 0 = unlimited)
    for i in 1..tiers.len() {
        let prev = &tiers[i - 1];
        let curr = &tiers[i];
        // Intermediate tiers must have non-zero up_to, and each must be greater than the previous
        if i < tiers.len() - 1 {
            require!(curr.up_to > prev.up_to, VelaError::InvalidTierBoundary);
        } else {
            // Last tier: up_to == 0 means unlimited; if non-zero, must be > previous
            if curr.up_to != 0 {
                require!(curr.up_to > prev.up_to, VelaError::InvalidTierBoundary);
            }
        }
    }

    let token_2022_program_id = spl_token_2022::id();
    require_keys_eq!(
        ctx.accounts.token_2022_program.key(),
        anchor_pubkey(token_2022_program_id)
    );

    let merchant_key = ctx.accounts.merchant.key();
    let merchant_state = ensure_merchant_state(
        &ctx.accounts.merchant.to_account_info(),
        &ctx.accounts.merchant_state.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.rent,
        &merchant_key,
        ctx.bumps.merchant_state,
    )?;
    let credential_mint_key = ctx.accounts.credential_mint.key();

    // Resolve credential mint: merchant-first (D-10), plan-scoped fallback (CRED-05)
    let resolved_credential_mint = resolve_merchant_credential_mint(
        &ctx.accounts.merchant_state.to_account_info(),
        &merchant_key,
        &credential_mint_key,
    )?;
    require!(
        merchant_state.credential_mint != Pubkey::default()
            && resolved_credential_mint == merchant_state.credential_mint,
        VelaError::MigrationPreconditionFailed
    );

    // Copy tiers into fixed array, zero-fill unused slots
    let mut tiers_array = [PricingTier::default(); 5];
    for (i, tier) in tiers.iter().enumerate() {
        tiers_array[i] = *tier;
    }

    ctx.accounts.usage_plan.set_inner(UsagePlan {
        merchant: merchant_key,
        plan_id,
        unit_name,
        tiers: tiers_array,
        tier_count: tiers.len() as u8,
        max_charge_per_period,
        settlement_frequency,
        credential_mint: resolved_credential_mint,
        status: PlanStatus::Active,
        bump: ctx.bumps.usage_plan,
        version: CURRENT_ACCOUNT_VERSION,
        _reserved: [0; ACCOUNT_RESERVED_BYTES],
    });

    Ok(())
}

fn anchor_pubkey(key: SplPubkey) -> Pubkey {
    Pubkey::new_from_array(key.to_bytes())
}
