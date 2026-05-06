use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use solana_instruction::{AccountMeta as SplAccountMeta, Instruction as SplInstruction};
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;

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
    protocol_program: &Pubkey,
    wrapping_vault: &Pubkey,
    protocol_config: &Pubkey,
    pull_approval: &Pubkey,
    token_config: &Pubkey,
    system_program: &Pubkey,
    extra_account_meta_list: &Pubkey,
    hook_program: &Pubkey,
) -> [SplAccountMeta; 10] {
    [
        SplAccountMeta::new_readonly(spl_pubkey(protocol_program), false),
        SplAccountMeta::new_readonly(spl_pubkey(wrapping_vault), false),
        SplAccountMeta::new_readonly(spl_pubkey(protocol_config), false),
        SplAccountMeta::new(spl_pubkey(pull_approval), false),
        SplAccountMeta::new_readonly(spl_pubkey(token_config), false),
        SplAccountMeta::new_readonly(spl_pubkey(system_program), false),
        SplAccountMeta::new_readonly(spl_pubkey(system_program), false),
        SplAccountMeta::new_readonly(spl_pubkey(system_program), false),
        SplAccountMeta::new_readonly(spl_pubkey(extra_account_meta_list), false),
        SplAccountMeta::new_readonly(spl_pubkey(hook_program), false),
    ]
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
        .extend_from_slice(&transfer_hook_account_metas(
            accounts.protocol_program.key,
            accounts.wrapping_vault.key,
            accounts.protocol_config.key,
            accounts.pull_approval.key,
            accounts.token_config.key,
            accounts.system_program.key,
            accounts.extra_account_meta_list.key,
            accounts.hook_program.key,
        ));

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
