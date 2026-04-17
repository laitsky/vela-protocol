use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::associated_token;
use solana_instruction::Instruction as SplInstruction;
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;
use spl_token_2022::instruction::burn;

use crate::{
    errors::VelaError,
    instructions::{
        mandate_account::{load_mandate_account, validate_loaded_mandate_address, write_mandate},
        merchant_account::resolve_merchant_credential_mint,
        plan_account::LoadedPlanAccount,
        plan_account::{load_plan_account, require_plan_billing_type},
        protocol_config_account::load_protocol_config,
    },
    state::{MandateStatus, MerchantState, ProtocolConfig, UsagePlan, VelaPlan},
};

#[derive(Accounts)]
pub struct AdminCancel<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    /// CHECK: Used to validate merchant_state PDA.
    pub merchant: UncheckedAccount<'info>,

    /// CHECK: MerchantState PDA for merchant-first credential resolution.
    #[account(seeds = [MerchantState::SEED_PREFIX, merchant.key().as_ref()], bump)]
    pub merchant_state: UncheckedAccount<'info>,

    /// CHECK: Validated in handler as mandate.subscriber.
    pub subscriber: UncheckedAccount<'info>,

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
}

pub fn handler(ctx: Context<AdminCancel>) -> Result<()> {
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require_keys_eq!(
        ctx.accounts.admin.key(),
        protocol_config.admin(),
        VelaError::UnauthorizedAdmin
    );
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
    require_keys_eq!(mandate.subscriber, ctx.accounts.subscriber.key());
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

    let merchant_key = plan.merchant();
    let plan_id_bytes = plan.plan_id().to_le_bytes();
    let plan_bump = [plan.bump()];
    let merchant_state_bump = [ctx.bumps.merchant_state];
    let (legacy_credential_seed_prefix, plan_seed_prefix) = match &plan {
        LoadedPlanAccount::Flat(_) | LoadedPlanAccount::LegacyFlat(_) => {
            (b"credential".as_slice(), VelaPlan::SEED_PREFIX)
        }
        LoadedPlanAccount::Usage(_) | LoadedPlanAccount::LegacyUsage(_) => {
            (b"usage_credential".as_slice(), UsagePlan::SEED_PREFIX)
        }
    };
    let (legacy_plan_credential_mint, _) = Pubkey::find_program_address(
        &[
            legacy_credential_seed_prefix,
            merchant_key.as_ref(),
            plan_id_bytes.as_ref(),
        ],
        &crate::ID,
    );
    let use_merchant_authority = resolved_credential_mint != legacy_plan_credential_mint;
    let signer_seeds: Vec<&[u8]> = if use_merchant_authority {
        vec![
            MerchantState::SEED_PREFIX,
            merchant_key.as_ref(),
            merchant_state_bump.as_ref(),
        ]
    } else {
        vec![
            plan_seed_prefix,
            merchant_key.as_ref(),
            plan_id_bytes.as_ref(),
            plan_bump.as_ref(),
        ]
    };
    let burn_authority = if use_merchant_authority {
        ctx.accounts.merchant_state.key()
    } else {
        ctx.accounts.plan.key()
    };

    let burn_ix = burn(
        &spl_token_2022::id(),
        &spl_pubkey(ctx.accounts.subscriber_credential_account.key()),
        &spl_pubkey(ctx.accounts.credential_mint.key()),
        &spl_pubkey(burn_authority),
        &[],
        1,
    )
    .map_err(map_interface_error)?;
    invoke_signed(
        &convert_instruction(burn_ix),
        &[
            ctx.accounts.subscriber_credential_account.to_account_info(),
            ctx.accounts.credential_mint.to_account_info(),
            if use_merchant_authority {
                ctx.accounts.merchant_state.to_account_info()
            } else {
                ctx.accounts.plan.to_account_info()
            },
        ],
        &[signer_seeds.as_slice()],
    )?;

    mandate.status = MandateStatus::Cancelled;
    write_mandate(
        &ctx.accounts.mandate.to_account_info(),
        &mandate,
        legacy_layout,
    )?;

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
