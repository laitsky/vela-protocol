use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::associated_token;
use solana_instruction::Instruction as SplInstruction;
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;
use spl_token_2022::instruction::burn;

use crate::{
    errors::VelaError,
    instructions::plan_account::{load_plan_account, require_plan_billing_type},
    state::{MandateStatus, ProtocolConfig, VelaMandate, VelaPlan},
};

#[derive(Accounts)]
pub struct AdminCancel<'info> {
    #[account(
        mut,
        constraint = admin.key() == protocol_config.admin @ VelaError::UnauthorizedAdmin
    )]
    pub admin: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// CHECK: Validated in handler as mandate.subscriber.
    pub subscriber: UncheckedAccount<'info>,

    /// CHECK: Deserialized and validated manually to support both flat and usage plans.
    pub plan: UncheckedAccount<'info>,

    #[account(mut)]
    pub mandate: Account<'info, VelaMandate>,

    #[account(mut)]
    /// CHECK: ATA address is validated against subscriber + credential mint in the handler.
    pub subscriber_credential_account: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Credential mint ownership is validated in the handler.
    pub credential_mint: UncheckedAccount<'info>,

    /// CHECK: Must be the Token-2022 program.
    pub token_2022_program: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<AdminCancel>) -> Result<()> {
    let plan = load_plan_account(&ctx.accounts.plan.to_account_info())?;
    require_plan_billing_type(&plan, &ctx.accounts.mandate.billing_type)?;
    require_keys_eq!(
        ctx.accounts.token_2022_program.key(),
        anchor_pubkey(spl_token_2022::id())
    );
    require_keys_eq!(ctx.accounts.mandate.plan, ctx.accounts.plan.key());
    require_keys_eq!(ctx.accounts.mandate.merchant, plan.merchant());
    require_keys_eq!(ctx.accounts.credential_mint.key(), plan.credential_mint());
    require_keys_eq!(
        ctx.accounts.mandate.subscriber,
        ctx.accounts.subscriber.key()
    );
    require!(
        ctx.accounts.mandate.status == MandateStatus::Active,
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

    // Burn the credential token using the plan PDA as authority via PermanentDelegate.
    // The PermanentDelegate extension grants the plan PDA the right to burn any holder's tokens.
    let merchant_key = plan.merchant();
    let plan_id_bytes = plan.plan_id().to_le_bytes();
    let plan_bump = [plan.bump()];
    let plan_signer_seeds: &[&[u8]] = &[
        VelaPlan::SEED_PREFIX,
        merchant_key.as_ref(),
        plan_id_bytes.as_ref(),
        &plan_bump,
    ];

    let burn_ix = burn(
        &spl_token_2022::id(),
        &spl_pubkey(ctx.accounts.subscriber_credential_account.key()),
        &spl_pubkey(ctx.accounts.credential_mint.key()),
        &spl_pubkey(ctx.accounts.plan.key()),
        &[],
        1,
    )
    .map_err(map_interface_error)?;
    invoke_signed(
        &convert_instruction(burn_ix),
        &[
            ctx.accounts.subscriber_credential_account.to_account_info(),
            ctx.accounts.credential_mint.to_account_info(),
            ctx.accounts.plan.to_account_info(),
        ],
        &[plan_signer_seeds],
    )?;

    ctx.accounts.mandate.status = MandateStatus::Cancelled;

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
