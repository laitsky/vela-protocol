use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
use solana_pubkey::Pubkey as SolPubkey;
use spl_tlv_account_resolution::{account::ExtraAccountMeta, state::ExtraAccountMetaList};
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::state::ProtocolConfig;

#[derive(Accounts)]
pub struct InitExtraAccountMetaList<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = config.bump,
        has_one = admin,
    )]
    pub config: Account<'info, ProtocolConfig>,

    /// The ExtraAccountMetaList PDA for the wrapped USDC mint.
    /// Seeds: [b"extra-account-metas", wrapped_usdc_mint.key()]
    ///
    /// CHECK: Initialized here with TLV data layout for Token-2022 hook resolution.
    #[account(
        init,
        payer = admin,
        space = ExtraAccountMetaList::size_of(2).unwrap() + 8,
        seeds = [b"extra-account-metas", wrapped_usdc_mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    pub wrapped_usdc_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Static wrapping vault address stored in the ExtraAccountMetaList.
    pub wrapping_vault: UncheckedAccount<'info>,

    /// CHECK: Static ProtocolConfig PDA address stored in the ExtraAccountMetaList.
    pub protocol_config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitExtraAccountMetaList>) -> Result<()> {
    // Build 2 static extra accounts:
    //   Account 5: wrapping_vault -- for wrap/unwrap bypass detection in transfer hook
    //   Account 6: protocol_config -- for future reference
    //
    // Note: PullApproval PDA is NOT in ExtraAccountMetaList because it varies per mandate.
    // The SDK passes PullApproval as a remaining account resolved per-transfer.
    //
    // Convert anchor Pubkeys to solana_pubkey::Pubkey (different crate, same bytes)
    let vault_key = SolPubkey::new_from_array(ctx.accounts.wrapping_vault.key().to_bytes());
    let config_key = SolPubkey::new_from_array(ctx.accounts.protocol_config.key().to_bytes());

    let account_metas = vec![
        ExtraAccountMeta::new_with_pubkey(&vault_key, false, false)
            .map_err(|_| anchor_lang::prelude::ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_with_pubkey(&config_key, false, false)
            .map_err(|_| anchor_lang::prelude::ProgramError::InvalidAccountData)?,
    ];

    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &account_metas,
    )
    .map_err(|_| anchor_lang::prelude::ProgramError::InvalidAccountData)?;

    Ok(())
}
