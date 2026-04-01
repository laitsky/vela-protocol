use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};
use solana_pubkey::Pubkey as SolPubkey;
use spl_discriminator::discriminator::SplDiscriminate;
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta,
    seeds::Seed,
    state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::{ExecuteInstruction, TransferHookInstruction};
use vela_protocol::{
    constants::EXTRA_ACCOUNT_METAS_SEED,
    errors::VelaError,
    state::{ProtocolConfig, PullApproval},
};

declare_id!("93q91TJ6M9yGoehAeeCttgEc1SThFGXaw4rZS2ysr3uX");

#[program]
pub mod vela_transfer_hook {
    use super::*;

    pub fn init_extra_account_meta_list(
        ctx: Context<InitExtraAccountMetaList>,
    ) -> Result<()> {
        handler_init_extra_account_meta_list(ctx)
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
            anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData)
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
        space = ExtraAccountMetaList::size_of(4).unwrap(),
        seeds = [EXTRA_ACCOUNT_METAS_SEED, wrapped_usdc_mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    pub wrapped_usdc_mint: InterfaceAccount<'info, Mint>,

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
    /// CHECK: Extra-account-meta PDA derived from the wrapped mint and hook program; stores only hook metadata.
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
}

fn handler_init_extra_account_meta_list(ctx: Context<InitExtraAccountMetaList>) -> Result<()> {
    let config = load_protocol_config(&ctx.accounts.config)?;
    require_keys_eq!(
        config.admin,
        ctx.accounts.admin.key(),
        VelaError::UnauthorizedAdmin
    );
    require_keys_eq!(
        ctx.accounts.wrapped_usdc_mint.key(),
        config.wrapped_usdc_mint,
        VelaError::WrappedMintNotInitialized
    );
    require_keys_eq!(
        ctx.accounts.wrapping_vault.key(),
        config.wrapping_vault,
        VelaError::VaultMismatch
    );

    let vault_key = SolPubkey::new_from_array(ctx.accounts.wrapping_vault.key().to_bytes());
    let config_key = SolPubkey::new_from_array(ctx.accounts.config.key().to_bytes());
    let protocol_program_key = SolPubkey::new_from_array(vela_protocol::ID.to_bytes());
    let account_metas = vec![
        ExtraAccountMeta::new_with_pubkey(&protocol_program_key, false, false)
            .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_with_pubkey(&vault_key, false, false)
            .map_err(|_| ProgramError::InvalidAccountData)?,
        ExtraAccountMeta::new_with_pubkey(&config_key, false, false)
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
    ];

    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &account_metas,
    )
    .map_err(|_| ProgramError::InvalidAccountData)?;

    Ok(())
}

fn handler_transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config)?;
    require_keys_eq!(
        ctx.accounts.mint.key(),
        protocol_config.wrapped_usdc_mint,
        VelaError::WrappedMintNotInitialized
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

    Ok(())
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
    ProtocolConfig::try_deserialize(&mut slice)
        .map_err(|_| VelaError::InvalidProtocolConfig.into())
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
