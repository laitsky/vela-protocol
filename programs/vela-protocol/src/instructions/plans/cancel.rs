use anchor_lang::{prelude::*, solana_program::program::invoke};
use anchor_spl::{
    associated_token,
    token::{revoke, Revoke},
};
use solana_program_error::ProgramError as SplProgramError;
use spl_token_2022::instruction::burn;

use crate::{
    errors::VelaError,
    instructions::{
        mandate_account::{load_mandate_account, validate_loaded_mandate_address, write_mandate},
        merchant_account::resolve_merchant_credential_mint,
        plan_account::{load_plan_account, require_plan_billing_type},
        spl_helpers::{
            anchor_pubkey, convert_instruction, map_spl_error_preserving_custom, spl_pubkey,
        },
    },
    state::{MandateStatus, MerchantState},
};

#[derive(Accounts)]
pub struct Cancel<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: Used for mandate and ATA validation.
    pub subscriber: UncheckedAccount<'info>,

    /// CHECK: Used to validate merchant_state PDA.
    pub merchant: UncheckedAccount<'info>,

    /// CHECK: MerchantState PDA for merchant-first credential resolution.
    #[account(seeds = [MerchantState::SEED_PREFIX, merchant.key().as_ref()], bump)]
    pub merchant_state: UncheckedAccount<'info>,

    /// CHECK: Deserialized and validated manually to support both flat and usage plans.
    pub plan: UncheckedAccount<'info>,

    #[account(mut)]
    pub mandate: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: ATA address is validated against subscriber + credential mint in the handler.
    pub subscriber_credential_account: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Credential mint ownership is validated in the handler.
    pub credential_mint: UncheckedAccount<'info>,

    /// CHECK: Must be the Token-2022 program.
    pub token_2022_program: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Legacy SPL-token approval account. Token-2022 billing rails do not require it.
    pub subscriber_token_account: UncheckedAccount<'info>,

    /// CHECK: Legacy SPL token program used only when subscriber_token_account is owned by it.
    pub token_program: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Cancel>) -> Result<()> {
    let loaded_mandate = load_mandate_account(&ctx.accounts.mandate.to_account_info())?;
    validate_loaded_mandate_address(&ctx.accounts.mandate.key(), &loaded_mandate)?;
    let legacy_layout = loaded_mandate.is_legacy();
    let mut mandate = loaded_mandate.into_current();
    let plan = load_plan_account(&ctx.accounts.plan.to_account_info())?;
    require_plan_billing_type(&plan, &mandate.billing_type)?;
    require_keys_eq!(
        ctx.accounts.token_2022_program.key(),
        anchor_pubkey(spl_token_2022::id())
    );
    require_keys_eq!(ctx.accounts.plan.key(), mandate.plan);
    require_keys_eq!(ctx.accounts.merchant.key(), plan.merchant());
    require_keys_eq!(mandate.merchant, plan.merchant());
    let resolved_credential_mint = resolve_merchant_credential_mint(
        &ctx.accounts.merchant_state.to_account_info(),
        &plan.merchant(),
        &plan.credential_mint(),
    )?;
    require_keys_eq!(ctx.accounts.credential_mint.key(), resolved_credential_mint);
    require!(
        mandate.subscriber == ctx.accounts.subscriber.key(),
        VelaError::UnauthorizedCancel
    );
    require!(
        ctx.accounts.authority.key() == ctx.accounts.subscriber.key(),
        VelaError::UnauthorizedCancel
    );
    require!(
        mandate.status == MandateStatus::Active,
        VelaError::MandateNotActive
    );

    let credential_ata = associated_token::get_associated_token_address_with_program_id(
        &ctx.accounts.subscriber.key(),
        &ctx.accounts.credential_mint.key(),
        &ctx.accounts.token_2022_program.key(),
    );
    require_keys_eq!(
        ctx.accounts.subscriber_credential_account.key(),
        credential_ata
    );

    let burn_ix = burn(
        &spl_token_2022::id(),
        &spl_pubkey(&ctx.accounts.subscriber_credential_account.key()),
        &spl_pubkey(&ctx.accounts.credential_mint.key()),
        &spl_pubkey(&ctx.accounts.authority.key()),
        &[],
        1,
    )
    .map_err(map_interface_error)?;
    invoke(
        &convert_instruction(burn_ix),
        &[
            ctx.accounts.subscriber_credential_account.to_account_info(),
            ctx.accounts.credential_mint.to_account_info(),
            ctx.accounts.authority.to_account_info(),
        ],
    )?;

    if ctx.accounts.subscriber_token_account.owner == ctx.accounts.token_program.key
        && !ctx.accounts.subscriber_token_account.data_is_empty()
    {
        let revoke_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Revoke {
                source: ctx.accounts.subscriber_token_account.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        );
        revoke(revoke_ctx)?;
    }

    mandate.status = MandateStatus::Cancelled;
    write_mandate(
        &ctx.accounts.mandate.to_account_info(),
        &mandate,
        legacy_layout,
    )?;

    Ok(())
}

fn map_interface_error(error: SplProgramError) -> Error {
    map_spl_error_preserving_custom(error)
}
