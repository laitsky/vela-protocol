use anchor_lang::prelude::*;

use crate::state::MerchantState;

#[derive(Accounts)]
pub struct InitMerchantCredential<'info> {
    #[account(mut)]
    pub merchant: Signer<'info>,

    #[account(mut, seeds = [MerchantState::SEED_PREFIX, merchant.key().as_ref()], bump)]
    pub merchant_state: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: PDA mint account created via manual CPI, constrained by seeds.
    pub credential_mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    /// CHECK: Address constraint ensures this is the Token-2022 program.
    pub token_2022_program: UncheckedAccount<'info>,

    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(_ctx: Context<InitMerchantCredential>) -> Result<()> {
    // Stub -- implementation in Task 2
    Ok(())
}
