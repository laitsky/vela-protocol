use crate::errors::VelaError;
use crate::instructions::arcium_accounts::{derive_mxe_pubkey, validate_cluster_configuration};
use crate::instructions::protocol_config_account::{
    load_protocol_config, upgrade_protocol_config, write_protocol_config,
};
use crate::state::{ClusterType, ProtocolConfig};
use anchor_lang::{
    prelude::*,
    solana_program::bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    AccountDeserialize,
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitConfigIx {
    pub cluster_pubkey: Pubkey,
    pub cluster_type: ClusterType,
    pub cluster_offset: u64,
    pub mxe_program_id: Pubkey,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdateConfigIx {
    pub cluster_pubkey: Pubkey,
    pub cluster_type: ClusterType,
    pub cluster_offset: u64,
    pub mxe_program_id: Pubkey,
}

pub fn init_config(ctx: Context<InitConfig>, ix: InitConfigIx) -> Result<()> {
    validate_program_upgrade_authority(
        &ctx.accounts.admin,
        &ctx.accounts.program,
        &ctx.accounts.program_data,
    )?;
    validate_cluster_configuration(&ix.cluster_pubkey, ix.cluster_offset)?;
    let mxe_program_id = normalize_mxe_program_id(ix.mxe_program_id);
    validate_mxe_account(&ctx.accounts.mxe_account, &mxe_program_id)?;
    let config = &mut ctx.accounts.config;
    **config = ProtocolConfig::new(
        ctx.accounts.admin.key(),
        ix.cluster_pubkey,
        ix.cluster_type,
        ix.cluster_offset,
        mxe_program_id,
        ctx.bumps.config,
    );
    Ok(())
}

fn normalize_mxe_program_id(mxe_program_id: Pubkey) -> Pubkey {
    if mxe_program_id == Pubkey::default() {
        crate::ID
    } else {
        mxe_program_id
    }
}

fn validate_mxe_account(mxe_account: &UncheckedAccount<'_>, mxe_program_id: &Pubkey) -> Result<()> {
    require_keys_eq!(
        mxe_account.key(),
        derive_mxe_pubkey(mxe_program_id),
        VelaError::InvalidProtocolConfig
    );
    Ok(())
}

fn validate_program_upgrade_authority(
    admin: &Signer<'_>,
    program: &UncheckedAccount<'_>,
    program_data: &UncheckedAccount<'_>,
) -> Result<()> {
    require_keys_eq!(program.key(), crate::ID, VelaError::InvalidProtocolConfig);
    require_keys_eq!(
        *program.owner,
        bpf_loader_upgradeable::ID,
        VelaError::InvalidProtocolConfig
    );
    require_keys_eq!(
        *program_data.owner,
        bpf_loader_upgradeable::ID,
        VelaError::InvalidProtocolConfig
    );

    let program_state = {
        let data = program.try_borrow_data()?;
        let mut slice: &[u8] = &data;
        UpgradeableLoaderState::try_deserialize_unchecked(&mut slice)
            .map_err(|_| VelaError::InvalidProtocolConfig)?
    };
    let expected_program_data = match program_state {
        UpgradeableLoaderState::Program {
            programdata_address,
        } => programdata_address,
        _ => return Err(VelaError::InvalidProtocolConfig.into()),
    };
    require_keys_eq!(
        program_data.key(),
        expected_program_data,
        VelaError::InvalidProtocolConfig
    );

    let program_data_state = {
        let data = program_data.try_borrow_data()?;
        let mut slice: &[u8] = &data;
        UpgradeableLoaderState::try_deserialize_unchecked(&mut slice)
            .map_err(|_| VelaError::InvalidProtocolConfig)?
    };
    let upgrade_authority = match program_data_state {
        UpgradeableLoaderState::ProgramData {
            upgrade_authority_address: Some(authority),
            ..
        } => authority,
        UpgradeableLoaderState::ProgramData {
            upgrade_authority_address: None,
            ..
        } => return Err(VelaError::UnauthorizedAdmin.into()),
        _ => return Err(VelaError::InvalidProtocolConfig.into()),
    };
    require_keys_eq!(admin.key(), upgrade_authority, VelaError::UnauthorizedAdmin);

    Ok(())
}

pub fn update_config(ctx: Context<UpdateConfig>, ix: UpdateConfigIx) -> Result<()> {
    validate_cluster_configuration(&ix.cluster_pubkey, ix.cluster_offset)?;
    let mxe_program_id = normalize_mxe_program_id(ix.mxe_program_id);
    validate_mxe_account(&ctx.accounts.mxe_account, &mxe_program_id)?;
    let existing = load_protocol_config(&ctx.accounts.config.to_account_info())?;
    require_keys_eq!(
        ctx.accounts.admin.key(),
        existing.admin(),
        VelaError::UnauthorizedAdmin
    );
    let mut config = upgrade_protocol_config(
        &ctx.accounts.admin.to_account_info(),
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
    )?;
    config.cluster_pubkey = ix.cluster_pubkey;
    config.cluster_type = ix.cluster_type;
    config.cluster_offset = ix.cluster_offset;
    config.mxe_program_id = mxe_program_id;
    write_protocol_config(&ctx.accounts.config.to_account_info(), &config)?;
    Ok(())
}

#[derive(Accounts)]
pub struct InitConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    /// CHECK: Validated as this program's executable upgradeable-loader account.
    #[account(address = crate::ID)]
    pub program: UncheckedAccount<'info>,
    /// CHECK: Validated against the program account and current upgrade authority.
    pub program_data: UncheckedAccount<'info>,
    /// CHECK: Validated as the Arcium MXE PDA for ix.mxe_program_id.
    pub mxe_account: UncheckedAccount<'info>,
    #[account(
        init,
        payer = admin,
        space = ProtocolConfig::SIZE,
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: Account<'info, ProtocolConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub config: UncheckedAccount<'info>,
    /// CHECK: Validated as the Arcium MXE PDA for ix.mxe_program_id.
    pub mxe_account: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
