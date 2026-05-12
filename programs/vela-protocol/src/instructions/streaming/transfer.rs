use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::{
        protocol_config_account::load_protocol_config,
        spl_helpers::{
            invoke_transfer_checked_with_hook, validate_token_2022_transfer_accounts,
            TransferCheckedWithHookAccounts,
        },
    },
    state::TokenConfig,
};

pub(crate) struct StreamTransferAccounts<'a, 'info> {
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
    pub expected_source_authority: Pubkey,
    pub expected_destination_owner: Pubkey,
}

pub(crate) fn validate_stream_transfer_accounts(
    protocol_config_info: &AccountInfo<'_>,
    hook_program_info: &AccountInfo<'_>,
    _wrapped_usdc_mint_info: &AccountInfo<'_>,
    wrapping_vault_info: &AccountInfo<'_>,
) -> Result<()> {
    let protocol_config = load_protocol_config(protocol_config_info)?;
    require!(!protocol_config.paused(), VelaError::ProtocolPaused);
    let hook_program_id = protocol_config.transfer_hook_program_id();
    require!(
        hook_program_id != Pubkey::default(),
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        hook_program_info.key(),
        hook_program_id,
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        wrapping_vault_info.key(),
        protocol_config.wrapping_vault(),
        VelaError::VaultMismatch
    );
    Ok(())
}

pub(crate) fn invoke_stream_transfer(
    accounts: StreamTransferAccounts<'_, '_>,
    amount: u64,
    signer_seed_groups: &[&[&[u8]]],
) -> Result<()> {
    validate_token_2022_transfer_accounts(
        accounts.source,
        accounts.destination,
        accounts.mint.key,
        accounts.token_2022_program.key,
        &accounts.expected_source_authority,
        &accounts.expected_destination_owner,
    )?;

    let decimals = {
        let data = accounts.token_config.try_borrow_data()?;
        let mut slice: &[u8] = &data;
        TokenConfig::try_deserialize(&mut slice)
            .map_err(|_| VelaError::TokenNotRegistered)?
            .decimals
    };
    invoke_transfer_checked_with_hook(
        TransferCheckedWithHookAccounts {
            source: accounts.source,
            mint: accounts.mint,
            destination: accounts.destination,
            authority: accounts.authority,
            protocol_program: accounts.protocol_program,
            wrapping_vault: accounts.wrapping_vault,
            protocol_config: accounts.protocol_config,
            pull_approval: accounts.pull_approval,
            token_config: accounts.token_config,
            system_program: accounts.system_program,
            extra_account_meta_list: accounts.extra_account_meta_list,
            hook_program: accounts.hook_program,
            token_2022_program: accounts.token_2022_program,
        },
        amount,
        decimals,
        signer_seed_groups,
    )
}
