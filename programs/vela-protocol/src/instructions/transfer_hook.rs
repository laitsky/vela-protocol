use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::{errors::VelaError, state::PullApproval};

#[derive(Accounts)]
pub struct TransferHook<'info> {
    // Accounts 0-3: Standard transfer accounts required by Token-2022 (fixed order)
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub destination_token: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Owner of source account, passed by Token-2022.
    pub owner: UncheckedAccount<'info>,

    // Account 4: ExtraAccountMetaList PDA (fixed position required by Token-2022 hook protocol)
    /// CHECK: ExtraAccountMetaList PDA, validated by seeds.
    #[account(
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    // Accounts 5-6: Static extra accounts from ExtraAccountMetaList
    /// CHECK: Wrapping vault address for wrap/unwrap bypass detection (from ExtraAccountMetaList).
    pub wrapping_vault: UncheckedAccount<'info>,

    /// CHECK: ProtocolConfig PDA (from ExtraAccountMetaList, for future reference).
    pub protocol_config: UncheckedAccount<'info>,

    // Account 7+: PullApproval PDA -- remaining account passed by the SDK per mandate
    /// CHECK: PullApproval PDA is validated manually in the handler (owner, data, fields).
    pub pull_approval: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
    // 1. Bypass check FIRST (Pitfall 5): allow wrap/unwrap transfers without a PullApproval.
    //    Wrap: subscriber deposits SPL USDC into vault (destination = vault)
    //    Unwrap: protocol releases SPL USDC from vault (source = vault)
    let vault_key = ctx.accounts.wrapping_vault.key();
    if ctx.accounts.source_token.key() == vault_key
        || ctx.accounts.destination_token.key() == vault_key
    {
        return Ok(());
    }

    // 2. Validate PullApproval PDA exists and is owned by this program.
    let approval_info = &ctx.accounts.pull_approval;
    if approval_info.owner != &crate::ID || approval_info.data_is_empty() {
        return Err(VelaError::TransferNotAuthorized.into());
    }

    // 3. Deserialize PullApproval and validate fields.
    //    Note: No CPIs are allowed here -- CPI depth is 3 when hook fires.
    //    (client -> execute_pull -> Token-2022 -> transfer hook)
    let approval_data = approval_info.try_borrow_data()?;
    let mut approval_slice: &[u8] = &approval_data;
    let approval = PullApproval::try_deserialize(&mut approval_slice)
        .map_err(|_| VelaError::TransferNotAuthorized)?;

    // 4. Validate approval fields
    require!(approval.approved, VelaError::ApprovalNotGranted);
    require!(
        Clock::get()?.unix_timestamp <= approval.valid_until,
        VelaError::ApprovalExpired
    );
    require!(
        amount <= approval.approved_amount,
        VelaError::AmountExceedsPlanAmount
    );

    Ok(())
}
