use anchor_lang::prelude::*;

use crate::{
    errors::VelaError,
    instructions::protocol_config_account::load_protocol_config,
    state::{ProtocolConfig, TokenConfig},
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdateTokenConfigIx {
    pub enabled: Option<bool>,
    pub oracle_reference: Option<Pubkey>,
}

#[derive(Accounts)]
pub struct UpdateTokenConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: Protocol config is versioned; loader validates PDA, owner, and layout.
    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [TokenConfig::SEED_PREFIX, token_config.mint.as_ref()],
        bump = token_config.bump,
    )]
    pub token_config: Account<'info, TokenConfig>,
}

#[event]
pub struct TokenConfigUpdated {
    pub mint: Pubkey,
    pub enabled: Option<bool>,
    pub oracle_reference: Option<Pubkey>,
}

pub fn handler(ctx: Context<UpdateTokenConfig>, ix: UpdateTokenConfigIx) -> Result<()> {
    require!(
        ix.enabled.is_some() || ix.oracle_reference.is_some(),
        VelaError::NoUpdateProvided
    );

    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require_keys_eq!(
        protocol_config.admin(),
        ctx.accounts.admin.key(),
        VelaError::UnauthorizedAdmin
    );

    let token_config = &mut ctx.accounts.token_config;

    if let Some(enabled) = ix.enabled {
        token_config.enabled = enabled;
    }

    if let Some(oracle_reference) = ix.oracle_reference {
        token_config.oracle_reference = oracle_reference;
    }

    emit!(TokenConfigUpdated {
        mint: token_config.mint,
        enabled: ix.enabled,
        oracle_reference: ix.oracle_reference,
    });

    Ok(())
}
