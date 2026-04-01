use crate::errors::VelaError;
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

    let config = &mut ctx.accounts.keeper_config;

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
        bump = protocol_config.bump,
        constraint = protocol_config.admin == admin.key() @ VelaError::UnauthorizedAdmin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    #[account(
        mut,
        seeds = [KeeperConfig::SEED_PREFIX],
        bump = keeper_config.bump,
    )]
    pub keeper_config: Account<'info, KeeperConfig>,
}
