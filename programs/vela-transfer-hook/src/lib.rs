use anchor_lang::{prelude::*, Discriminator};
use anchor_spl::token_interface::{Mint, TokenAccount};
use solana_pubkey::Pubkey as SolPubkey;
use spl_discriminator::discriminator::SplDiscriminate;
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::{ExecuteInstruction, TransferHookInstruction};
use vela_protocol::{
    constants::EXTRA_ACCOUNT_METAS_SEED,
    errors::VelaError,
    state::{BillingRail, ProtocolConfig, PullApproval, StreamMandate, StreamStatus, TokenConfig},
};

declare_id!("3agVoFp4NZFuKbVqCV8HbjSZn1xW4Utk4U1Wir3TKjZ9");

#[program]
pub mod vela_transfer_hook {
    use super::*;

    pub fn init_extra_account_meta_list(ctx: Context<InitExtraAccountMetaList>) -> Result<()> {
        handler_init_extra_account_meta_list(ctx)
    }

    pub fn update_extra_account_meta_list(ctx: Context<UpdateExtraAccountMetaList>) -> Result<()> {
        handler_update_extra_account_meta_list(ctx)
    }

    #[instruction(discriminator = ExecuteInstruction::SPL_DISCRIMINATOR_SLICE)]
    pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
        handler_transfer_hook(ctx, amount)
    }

    pub fn fallback<'info>(
        program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        let instruction = TransferHookInstruction::unpack(data).map_err(|_| {
            anchor_lang::error::Error::from(
                anchor_lang::prelude::ProgramError::InvalidInstructionData,
            )
        })?;
        match instruction {
            TransferHookInstruction::Execute { amount } => {
                let amount_bytes = amount.to_le_bytes();
                __private::__global::transfer_hook(program_id, accounts, &amount_bytes)
            }
            _ => Err(anchor_lang::error::Error::from(
                anchor_lang::prelude::ProgramError::InvalidInstructionData,
            )),
        }
    }
}

#[derive(Accounts)]
pub struct InitExtraAccountMetaList<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: Foreign-owned protocol config PDA from the main program. Validated manually.
    pub config: UncheckedAccount<'info>,

    #[account(
        init,
        payer = admin,
        space = ExtraAccountMetaList::size_of(8).unwrap(),
        seeds = [EXTRA_ACCOUNT_METAS_SEED, mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Static wrapping vault address stored in the ExtraAccountMetaList.
    pub wrapping_vault: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateExtraAccountMetaList<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: Foreign-owned protocol config PDA from the main program. Validated manually.
    pub config: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [EXTRA_ACCOUNT_METAS_SEED, mint.key().as_ref()],
        bump,
    )]
    /// CHECK: Existing ExtraAccountMetaList PDA to be resized and rewritten in place.
    pub extra_account_meta_list: UncheckedAccount<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: Static wrapping vault address stored in the ExtraAccountMetaList.
    pub wrapping_vault: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferHook<'info> {
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    pub destination_token: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Owner of source account, passed by Token-2022.
    pub owner: UncheckedAccount<'info>,

    #[account(
        seeds = [EXTRA_ACCOUNT_METAS_SEED, mint.key().as_ref()],
        bump,
    )]
    /// CHECK: Extra-account-meta PDA derived from the mint and hook program; stores only hook metadata.
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// CHECK: Main protocol executable, required to resolve external PDAs owned by it.
    #[account(address = vela_protocol::ID)]
    pub protocol_program: UncheckedAccount<'info>,

    /// CHECK: Wrapping vault address for wrap/unwrap bypass detection.
    pub wrapping_vault: UncheckedAccount<'info>,

    /// CHECK: Foreign-owned protocol config PDA from the main program. Validated manually.
    pub protocol_config: UncheckedAccount<'info>,

    /// CHECK: PullApproval PDA is validated manually for fail-closed behavior.
    pub pull_approval: UncheckedAccount<'info>,

    /// CHECK: TokenConfig PDA is validated manually for fail-closed behavior.
    pub token_config: UncheckedAccount<'info>,

    /// CHECK: Reserved slot placeholder, ignored by the handler.
    pub _reserved_slot_6: UncheckedAccount<'info>,

    /// CHECK: Reserved slot placeholder, ignored by the handler.
    pub _reserved_slot_7: UncheckedAccount<'info>,

    /// CHECK: Reserved slot placeholder, ignored by the handler.
    pub _reserved_slot_8: UncheckedAccount<'info>,
}

fn handler_init_extra_account_meta_list(ctx: Context<InitExtraAccountMetaList>) -> Result<()> {
    let config = load_protocol_config(&ctx.accounts.config)?;
    require_keys_eq!(
        config.admin,
        ctx.accounts.admin.key(),
        VelaError::UnauthorizedAdmin
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.key(),
        config.wrapping_vault,
        VelaError::VaultMismatch
    );

    let vault_key = SolPubkey::new_from_array(ctx.accounts.wrapping_vault.key().to_bytes());
    let config_key = SolPubkey::new_from_array(ctx.accounts.config.key().to_bytes());
    let protocol_program_key = SolPubkey::new_from_array(vela_protocol::ID.to_bytes());
    let account_metas = build_8_slot_account_metas(&protocol_program_key, &vault_key, &config_key)?;

    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &account_metas,
    )
    .map_err(|_| ProgramError::InvalidAccountData)?;

    Ok(())
}

fn handler_update_extra_account_meta_list(ctx: Context<UpdateExtraAccountMetaList>) -> Result<()> {
    let config = load_protocol_config(&ctx.accounts.config)?;
    require_keys_eq!(
        config.admin,
        ctx.accounts.admin.key(),
        VelaError::UnauthorizedAdmin
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.key(),
        config.wrapping_vault,
        VelaError::VaultMismatch
    );

    let meta_list_info = ctx.accounts.extra_account_meta_list.to_account_info();
    let new_size = ExtraAccountMetaList::size_of(8).unwrap();
    require!(
        meta_list_info.data_len() < new_size,
        VelaError::MetaListAlreadyInitialized
    );

    let vault_key = SolPubkey::new_from_array(ctx.accounts.wrapping_vault.key().to_bytes());
    let config_key = SolPubkey::new_from_array(ctx.accounts.config.key().to_bytes());
    let protocol_program_key = SolPubkey::new_from_array(vela_protocol::ID.to_bytes());
    let account_metas = build_8_slot_account_metas(&protocol_program_key, &vault_key, &config_key)?;

    let rent = Rent::get()?;
    let required_lamports = rent.minimum_balance(new_size);
    let current_lamports = meta_list_info.lamports();

    if required_lamports > current_lamports {
        let diff = required_lamports - current_lamports;
        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                &ctx.accounts.admin.key(),
                meta_list_info.key,
                diff,
            ),
            &[
                ctx.accounts.admin.to_account_info(),
                meta_list_info.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;
    }

    meta_list_info.resize(new_size)?;
    {
        let mut data = meta_list_info.try_borrow_mut_data()?;
        data.fill(0);
        ExtraAccountMetaList::init::<ExecuteInstruction>(&mut data, &account_metas)
            .map_err(|_| ProgramError::InvalidAccountData)?;
    }

    Ok(())
}

fn handler_transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config)?;
    let token_config = load_token_config(&ctx.accounts.token_config, &ctx.accounts.mint.key())?;
    require_keys_eq!(
        token_config.mint,
        ctx.accounts.mint.key(),
        VelaError::TokenNotRegistered
    );
    require!(token_config.enabled, VelaError::TokenDisabled);
    require!(
        token_config.billing_rail == BillingRail::TransferHook,
        VelaError::InvalidBillingRail
    );
    require_keys_eq!(
        ctx.accounts.source_token.mint,
        ctx.accounts.mint.key(),
        VelaError::TransferNotAuthorized
    );
    require_keys_eq!(
        ctx.accounts.destination_token.mint,
        ctx.accounts.mint.key(),
        VelaError::TransferNotAuthorized
    );
    require_keys_eq!(
        ctx.accounts.source_token.owner,
        ctx.accounts.owner.key(),
        VelaError::TransferNotAuthorized
    );

    if ctx.accounts.mint.key() == protocol_config.wrapped_usdc_mint {
        require_keys_eq!(
            ctx.accounts.wrapping_vault.key(),
            protocol_config.wrapping_vault,
            VelaError::VaultMismatch
        );
        if ctx.accounts.source_token.key() == protocol_config.wrapping_vault
            || ctx.accounts.destination_token.key() == protocol_config.wrapping_vault
        {
            return Ok(());
        }
    }

    if ctx.accounts.owner.owner == &vela_protocol::ID && !ctx.accounts.owner.data_is_empty() {
        let owner_data = ctx.accounts.owner.try_borrow_data()?;
        if owner_data.len() < 8 {
            return Err(ProgramError::InvalidAccountData.into());
        }
        // Phase 45 pre-gate GO decision: keep slot 3 derived as PullApproval for TLV compatibility,
        // and dispatch the streaming branch from the transfer authority (owner) discriminator first.
        if &owner_data[..8] == StreamMandate::DISCRIMINATOR {
            let mut stream_slice: &[u8] = &owner_data;
            let stream = StreamMandate::try_deserialize(&mut stream_slice)
                .map_err(|_| VelaError::WrongAccountType)?;
            require!(
                stream.status == StreamStatus::Active,
                VelaError::StreamNotActive
            );
            let elapsed_i64 = Clock::get()?
                .unix_timestamp
                .checked_sub(stream.last_settled_ts)
                .ok_or(VelaError::ClockRegression)?;
            require!(elapsed_i64 >= 0, VelaError::ClockRegression);
            require!(
                elapsed_i64 >= i64::from(stream.min_settle_interval),
                VelaError::MinSettleIntervalViolation
            );
            let elapsed = u128::from(u64::try_from(elapsed_i64).map_err(|_| VelaError::Overflow)?);
            let gross = elapsed
                .checked_mul(u128::from(stream.rate_per_second))
                .ok_or(VelaError::Overflow)?;
            require!(
                u128::from(amount) <= gross,
                VelaError::AmountExceedsStreamRate
            );
            if let Some(cap) = stream.max_streamed {
                let next_total = u128::from(stream.total_streamed)
                    .checked_add(u128::from(amount))
                    .ok_or(VelaError::Overflow)?;
                require!(next_total <= u128::from(cap), VelaError::StreamCapExceeded);
            }
            return Ok(());
        }
    }

    let (expected_approval, _) = Pubkey::find_program_address(
        &[PullApproval::SEED_PREFIX, ctx.accounts.owner.key().as_ref()],
        &vela_protocol::ID,
    );
    require_keys_eq!(
        ctx.accounts.pull_approval.key(),
        expected_approval,
        VelaError::TransferNotAuthorized
    );

    let approval_info = &ctx.accounts.pull_approval;
    if approval_info.owner != &vela_protocol::ID || approval_info.data_is_empty() {
        return Err(VelaError::TransferNotAuthorized.into());
    }

    let approval_data = approval_info.try_borrow_data()?;
    require!(approval_data.len() >= 8, VelaError::WrongAccountType);
    if &approval_data[..8] == PullApproval::DISCRIMINATOR {
        let mut approval_slice: &[u8] = &approval_data;
        let approval = PullApproval::try_deserialize(&mut approval_slice)
            .map_err(|_| VelaError::TransferNotAuthorized)?;

        require_keys_eq!(
            approval.mandate,
            ctx.accounts.owner.key(),
            VelaError::TransferNotAuthorized
        );
        require!(approval.approved, VelaError::ApprovalNotGranted);
        require!(
            Clock::get()?.unix_timestamp <= approval.valid_until,
            VelaError::ApprovalExpired
        );
        require!(
            amount <= approval.approved_amount,
            VelaError::AmountExceedsPlanAmount
        );
        return Ok(());
    }

    Err(VelaError::WrongAccountType.into())
}

fn load_protocol_config(account: &UncheckedAccount<'_>) -> Result<ProtocolConfig> {
    let (expected_config, _) =
        Pubkey::find_program_address(&[ProtocolConfig::SEED_PREFIX], &vela_protocol::ID);
    require_keys_eq!(
        account.key(),
        expected_config,
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        *account.owner,
        vela_protocol::ID,
        VelaError::InvalidProtocolConfig
    );
    if account.data_is_empty() {
        return Err(VelaError::InvalidProtocolConfig.into());
    }

    let data = account.try_borrow_data()?;
    let mut slice: &[u8] = &data;
    ProtocolConfig::try_deserialize(&mut slice).map_err(|_| VelaError::InvalidProtocolConfig.into())
}

fn load_token_config(account: &UncheckedAccount<'_>, mint: &Pubkey) -> Result<TokenConfig> {
    let (expected_config, _) = Pubkey::find_program_address(
        &[TokenConfig::SEED_PREFIX, mint.as_ref()],
        &vela_protocol::ID,
    );
    require_keys_eq!(
        account.key(),
        expected_config,
        VelaError::TokenNotRegistered
    );
    require_keys_eq!(
        *account.owner,
        vela_protocol::ID,
        VelaError::TokenNotRegistered
    );
    if account.data_is_empty() {
        return Err(VelaError::TokenNotRegistered.into());
    }

    let data = account.try_borrow_data()?;
    let mut slice: &[u8] = &data;
    TokenConfig::try_deserialize(&mut slice).map_err(|_| VelaError::TokenNotRegistered.into())
}

/// Builds 8 entries:
/// 1. protocol program
/// 2. wrapping vault
/// 3. protocol config
/// 4. PullApproval PDA derived from owner
/// 5. TokenConfig PDA derived from mint
/// 6-8. reserved system program placeholders
fn build_8_slot_account_metas(
    protocol_program_key: &SolPubkey,
    vault_key: &SolPubkey,
    config_key: &SolPubkey,
) -> Result<Vec<ExtraAccountMeta>> {
    let system_program_key = SolPubkey::new_from_array(anchor_lang::system_program::ID.to_bytes());

    Ok(vec![
        ExtraAccountMeta::new_with_pubkey(protocol_program_key, false, false)
            .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_with_pubkey(vault_key, false, false)
            .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_with_pubkey(config_key, false, false)
            .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_external_pda_with_seeds(
            5,
            &[
                Seed::Literal {
                    bytes: PullApproval::SEED_PREFIX.to_vec(),
                },
                Seed::AccountKey { index: 3 },
            ],
            false,
            true,
        )
        .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_external_pda_with_seeds(
            5,
            &[
                Seed::Literal {
                    bytes: TokenConfig::SEED_PREFIX.to_vec(),
                },
                Seed::AccountKey { index: 1 },
            ],
            false,
            false,
        )
        .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_with_pubkey(&system_program_key, false, false)
            .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_with_pubkey(&system_program_key, false, false)
            .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_with_pubkey(&system_program_key, false, false)
            .map_err(|_| ProgramError::InvalidAccountData)?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use vela_protocol::constants::{TRANSFER_HOOK_PROGRAM_ID, TRANSFER_HOOK_PROGRAM_ID_BYTES};

    #[test]
    fn transfer_hook_program_id_constant_matches_declared_id() {
        assert_eq!(TRANSFER_HOOK_PROGRAM_ID, ID);
        assert_eq!(TRANSFER_HOOK_PROGRAM_ID_BYTES, ID.to_bytes());
    }
}
