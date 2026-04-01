use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::{
    associated_token::{self, AssociatedToken},
    token::Token,
};
use solana_instruction::Instruction as SplInstruction;
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;
use spl_token_2022::instruction::mint_to;

use crate::{
    errors::VelaError,
    state::{BillingType, MandateStatus, PlanStatus, VelaMandate, VelaPlan},
};

#[derive(Accounts)]
pub struct Subscribe<'info> {
    #[account(mut)]
    pub subscriber: Signer<'info>,

    /// CHECK: Used for plan and mint PDA validation only.
    pub merchant: UncheckedAccount<'info>,

    #[account(
        seeds = [
            VelaPlan::SEED_PREFIX,
            merchant.key().as_ref(),
            plan.plan_id.to_le_bytes().as_ref()
        ],
        bump = plan.bump
    )]
    pub plan: Account<'info, VelaPlan>,

    #[account(
        init,
        payer = subscriber,
        space = VelaMandate::SIZE,
        seeds = [
            VelaMandate::SEED_PREFIX,
            subscriber.key().as_ref(),
            plan.key().as_ref()
        ],
        bump
    )]
    pub mandate: Account<'info, VelaMandate>,

    #[account(mut, address = plan.credential_mint)]
    /// CHECK: Credential mint PDA ownership is validated in the handler.
    pub credential_mint: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: ATA address is validated against subscriber + credential mint in the handler.
    pub subscriber_credential_account: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,

    /// CHECK: Must be the Token-2022 program.
    pub token_2022_program: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<Subscribe>) -> Result<()> {
    require!(
        ctx.accounts.plan.status == PlanStatus::Active,
        VelaError::PlanNotActive
    );
    require_keys_eq!(
        ctx.accounts.token_2022_program.key(),
        anchor_pubkey(spl_token_2022::id())
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

    // No delegate approval needed -- subscribers no longer approve a delegate authority.
    // The mandate PDA is the authority over the subscriber's wrapped USDC account, and
    // transfer hook validation enforces all billing constraints (D-12, D-14).
    // Subscribers must wrap SPL USDC before executing pulls (use SDK wrapAndSubscribe).

    let ata_ctx = CpiContext::new(
        ctx.accounts.associated_token_program.to_account_info(),
        associated_token::Create {
            payer: ctx.accounts.subscriber.to_account_info(),
            associated_token: ctx.accounts.subscriber_credential_account.to_account_info(),
            authority: ctx.accounts.subscriber.to_account_info(),
            mint: ctx.accounts.credential_mint.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_2022_program.to_account_info(),
        },
    );
    associated_token::create_idempotent(ata_ctx)?;

    let merchant_key = ctx.accounts.merchant.key();
    let plan_id_bytes = ctx.accounts.plan.plan_id.to_le_bytes();
    let signer_seeds: &[&[u8]] = &[
        VelaPlan::SEED_PREFIX,
        merchant_key.as_ref(),
        plan_id_bytes.as_ref(),
        &[ctx.accounts.plan.bump],
    ];
    let mint_ix = mint_to(
        &spl_token_2022::id(),
        &spl_pubkey(ctx.accounts.credential_mint.key()),
        &spl_pubkey(ctx.accounts.subscriber_credential_account.key()),
        &spl_pubkey(ctx.accounts.plan.key()),
        &[],
        1,
    )
    .map_err(map_interface_error)?;
    invoke_signed(
        &convert_instruction(mint_ix),
        &[
            ctx.accounts.credential_mint.to_account_info(),
            ctx.accounts.subscriber_credential_account.to_account_info(),
            ctx.accounts.plan.to_account_info(),
        ],
        &[signer_seeds],
    )?;

    let clock = Clock::get()?;
    let frequency = i64::try_from(ctx.accounts.plan.frequency).map_err(|_| VelaError::Overflow)?;
    let trial_period =
        i64::try_from(ctx.accounts.plan.trial_period).map_err(|_| VelaError::Overflow)?;
    let expiry = if ctx.accounts.plan.trial_period > 0 {
        let total_duration = ctx
            .accounts
            .plan
            .trial_period
            .checked_add(
                ctx.accounts
                    .plan
                    .frequency
                    .checked_mul(ctx.accounts.plan.max_pulls)
                    .ok_or(VelaError::Overflow)?,
            )
            .ok_or(VelaError::Overflow)?;
        let total_duration = i64::try_from(total_duration).map_err(|_| VelaError::Overflow)?;
        clock
            .unix_timestamp
            .checked_add(total_duration)
            .ok_or(VelaError::Overflow)?
    } else {
        0
    };
    let next_payment_due = if ctx.accounts.plan.trial_period > 0 {
        clock
            .unix_timestamp
            .checked_add(trial_period)
            .ok_or(VelaError::Overflow)?
    } else {
        clock
            .unix_timestamp
            .checked_add(frequency)
            .ok_or(VelaError::Overflow)?
    };

    ctx.accounts.mandate.set_inner(VelaMandate {
        subscriber: ctx.accounts.subscriber.key(),
        plan: ctx.accounts.plan.key(),
        merchant: ctx.accounts.plan.merchant,
        amount: ctx.accounts.plan.amount,
        frequency: ctx.accounts.plan.frequency,
        start_date: clock.unix_timestamp,
        expiry,
        max_pulls: ctx.accounts.plan.max_pulls,
        pulls_executed: 0,
        next_payment_due,
        last_pull_at: 0,
        last_billing_recorded_pull: 0,
        validation_request_nonce: 0,
        billing_request_nonce: 0,
        status: MandateStatus::Active,
        bump: ctx.bumps.mandate,
        billing_type: BillingType::Flat,
    });

    Ok(())
}

fn spl_pubkey(key: Pubkey) -> SplPubkey {
    SplPubkey::from(key.to_bytes())
}

fn anchor_pubkey(key: SplPubkey) -> Pubkey {
    Pubkey::new_from_array(key.to_bytes())
}

fn convert_instruction(
    ix: SplInstruction,
) -> anchor_lang::solana_program::instruction::Instruction {
    anchor_lang::solana_program::instruction::Instruction {
        program_id: anchor_pubkey(ix.program_id),
        accounts: ix
            .accounts
            .into_iter()
            .map(|meta| {
                if meta.is_writable {
                    anchor_lang::solana_program::instruction::AccountMeta::new(
                        anchor_pubkey(meta.pubkey),
                        meta.is_signer,
                    )
                } else {
                    anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
                        anchor_pubkey(meta.pubkey),
                        meta.is_signer,
                    )
                }
            })
            .collect(),
        data: ix.data,
    }
}

fn map_interface_error(error: SplProgramError) -> Error {
    match error {
        SplProgramError::Custom(code) => {
            Error::from(anchor_lang::prelude::ProgramError::Custom(code))
        }
        _ => Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData),
    }
}
