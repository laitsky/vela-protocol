use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use solana_instruction::{AccountMeta as SplAccountMeta, Instruction as SplInstruction};
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;
use spl_token_2022::{extension::StateWithExtensions, state::Account as Token2022Account};

use crate::errors::VelaError;

pub(crate) struct TransferCheckedWithHookAccounts<'a, 'info> {
    pub source: &'a AccountInfo<'info>,
    pub mint: &'a AccountInfo<'info>,
    pub destination: &'a AccountInfo<'info>,
    pub authority: &'a AccountInfo<'info>,
    pub protocol_program: &'a AccountInfo<'info>,
    pub wrapping_vault: &'a AccountInfo<'info>,
    pub protocol_config: &'a AccountInfo<'info>,
    pub pull_approval: &'a AccountInfo<'info>,
    pub token_config: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
    pub extra_account_meta_list: &'a AccountInfo<'info>,
    pub hook_program: &'a AccountInfo<'info>,
    pub token_2022_program: &'a AccountInfo<'info>,
}

pub(crate) fn spl_pubkey(key: &Pubkey) -> SplPubkey {
    SplPubkey::from(key.to_bytes())
}

pub(crate) fn anchor_pubkey(key: SplPubkey) -> Pubkey {
    Pubkey::new_from_array(key.to_bytes())
}

pub(crate) fn convert_instruction(
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

pub(crate) fn map_spl_error_to_invalid_instruction(
    _error: SplProgramError,
) -> anchor_lang::error::Error {
    anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData)
}

pub(crate) fn map_spl_error_preserving_custom(error: SplProgramError) -> anchor_lang::error::Error {
    match error {
        SplProgramError::Custom(code) => {
            anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::Custom(code))
        }
        _ => anchor_lang::error::Error::from(
            anchor_lang::prelude::ProgramError::InvalidInstructionData,
        ),
    }
}

pub(crate) fn transfer_hook_account_metas(
    accounts: &TransferCheckedWithHookAccounts<'_, '_>,
) -> [SplAccountMeta; 10] {
    [
        SplAccountMeta::new_readonly(spl_pubkey(accounts.protocol_program.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(accounts.wrapping_vault.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(accounts.protocol_config.key), false),
        SplAccountMeta::new(spl_pubkey(accounts.pull_approval.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(accounts.token_config.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(accounts.system_program.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(accounts.system_program.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(accounts.system_program.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(accounts.extra_account_meta_list.key), false),
        SplAccountMeta::new_readonly(spl_pubkey(accounts.hook_program.key), false),
    ]
}

pub(crate) fn validate_token_2022_transfer_accounts(
    source: &AccountInfo<'_>,
    destination: &AccountInfo<'_>,
    mint: &Pubkey,
    token_2022_program: &Pubkey,
    expected_source_authority: &Pubkey,
    expected_destination_owner: &Pubkey,
) -> Result<()> {
    require_keys_eq!(
        *source.owner,
        *token_2022_program,
        VelaError::TransferNotAuthorized
    );
    require_keys_eq!(
        *destination.owner,
        *token_2022_program,
        VelaError::TransferNotAuthorized
    );

    let source_data = source.try_borrow_data()?;
    let source_account = StateWithExtensions::<Token2022Account>::unpack(&source_data)
        .map_err(|_| error!(VelaError::TransferNotAuthorized))?;
    let destination_data = destination.try_borrow_data()?;
    let destination_account = StateWithExtensions::<Token2022Account>::unpack(&destination_data)
        .map_err(|_| error!(VelaError::TransferNotAuthorized))?;

    let source_mint = Pubkey::new_from_array(source_account.base.mint.to_bytes());
    let destination_mint = Pubkey::new_from_array(destination_account.base.mint.to_bytes());
    let source_owner = Pubkey::new_from_array(source_account.base.owner.to_bytes());
    let destination_owner = Pubkey::new_from_array(destination_account.base.owner.to_bytes());

    require_keys_eq!(source_mint, *mint, VelaError::UsdcMintMismatch);
    require_keys_eq!(destination_mint, *mint, VelaError::UsdcMintMismatch);
    require_keys_eq!(
        source_owner,
        *expected_source_authority,
        VelaError::TransferNotAuthorized
    );
    require_keys_eq!(
        destination_owner,
        *expected_destination_owner,
        VelaError::TransferNotAuthorized
    );

    Ok(())
}

pub(crate) fn invoke_transfer_checked_with_hook(
    accounts: TransferCheckedWithHookAccounts<'_, '_>,
    amount: u64,
    decimals: u8,
    signer_seed_groups: &[&[&[u8]]],
) -> Result<()> {
    let mut transfer_ix = spl_token_2022::instruction::transfer_checked(
        &spl_pubkey(accounts.token_2022_program.key),
        &spl_pubkey(accounts.source.key),
        &spl_pubkey(accounts.mint.key),
        &spl_pubkey(accounts.destination.key),
        &spl_pubkey(accounts.authority.key),
        &[],
        amount,
        decimals,
    )
    .map_err(map_spl_error_to_invalid_instruction)?;

    transfer_ix
        .accounts
        .extend_from_slice(&transfer_hook_account_metas(&accounts));

    let transfer_ix = convert_instruction(transfer_ix);
    let transfer_account_infos = [
        accounts.source.clone(),
        accounts.mint.clone(),
        accounts.destination.clone(),
        accounts.authority.clone(),
        accounts.protocol_program.clone(),
        accounts.wrapping_vault.clone(),
        accounts.protocol_config.clone(),
        accounts.pull_approval.clone(),
        accounts.token_config.clone(),
        accounts.system_program.clone(),
        accounts.system_program.clone(),
        accounts.system_program.clone(),
        accounts.extra_account_meta_list.clone(),
        accounts.hook_program.clone(),
    ];
    invoke_signed(&transfer_ix, &transfer_account_infos, signer_seed_groups).map_err(Into::into)
}
