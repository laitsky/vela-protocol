use crate::errors::VelaError;
use crate::instructions::keeper_config_account::{
    load_keeper_config, upgrade_keeper_config, write_keeper_config,
};
use crate::instructions::protocol_config_account::load_protocol_config;
use crate::state::{KeeperConfig, KeeperMode, ProtocolConfig};
use anchor_lang::prelude::*;

pub fn handler(
    ctx: Context<UpdateKeeperConfig>,
    mode: Option<KeeperMode>,
    keeper_endpoint: Option<Vec<u8>>,
    keeper_authority: Option<Pubkey>,
) -> Result<()> {
    require!(
        mode.is_some() || keeper_endpoint.is_some() || keeper_authority.is_some(),
        VelaError::NoUpdateProvided
    );
    if let Some(ref ep) = keeper_endpoint {
        require!(ep.len() <= 128, VelaError::EndpointTooLong);
        require!(!ep.is_empty(), VelaError::EndpointEmpty);
    }
    if let Some(ka) = keeper_authority {
        require!(ka != Pubkey::default(), VelaError::InvalidKeeperAuthority);
    }

    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require_keys_eq!(
        protocol_config.admin(),
        ctx.accounts.admin.key(),
        VelaError::UnauthorizedAdmin
    );
    let _existing_keeper = load_keeper_config(&ctx.accounts.keeper_config.to_account_info())?;
    let mut config = upgrade_keeper_config(
        &ctx.accounts.admin.to_account_info(),
        &ctx.accounts.keeper_config.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
    )?;

    if let Some(m) = mode {
        config.mode = m.clone();
    }
    if let Some(ep) = keeper_endpoint {
        config.endpoint_len = ep.len() as u8;
        config.keeper_endpoint = [0u8; 128];
        config.keeper_endpoint[..ep.len()].copy_from_slice(&ep);
    }
    if let Some(ka) = keeper_authority {
        config.keeper_authority = ka;
    }

    emit!(KeeperConfigUpdated {
        admin: ctx.accounts.admin.key(),
        mode: config.mode.clone(),
        keeper_authority: config.keeper_authority,
    });
    write_keeper_config(&ctx.accounts.keeper_config.to_account_info(), &config)?;

    Ok(())
}

#[event]
pub struct KeeperConfigUpdated {
    pub admin: Pubkey,
    pub mode: KeeperMode,
    pub keeper_authority: Pubkey,
}

#[derive(Accounts)]
pub struct UpdateKeeperConfig<'info> {
    pub admin: Signer<'info>,
    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [KeeperConfig::SEED_PREFIX],
        bump,
    )]
    pub keeper_config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
