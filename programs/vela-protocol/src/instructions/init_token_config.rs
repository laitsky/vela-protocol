use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::{
    errors::VelaError,
    instructions::protocol_config_account::load_protocol_config,
    state::{
        BillingRail, ProtocolConfig, TokenConfig, ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION,
    },
};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitTokenConfigIx {
    pub billing_rail: BillingRail,
    pub decimals: u8,
}

#[derive(Accounts)]
pub struct InitTokenConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: Protocol config is versioned; loader validates PDA, owner, and layout.
    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump,
    )]
    pub protocol_config: UncheckedAccount<'info>,

    /// The token mint to register.
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        space = TokenConfig::SIZE,
        seeds = [TokenConfig::SEED_PREFIX, mint.key().as_ref()],
        bump,
    )]
    pub token_config: Account<'info, TokenConfig>,

    pub system_program: Program<'info, System>,
}

#[event]
pub struct TokenConfigCreated {
    pub mint: Pubkey,
    pub billing_rail: BillingRail,
    pub decimals: u8,
    pub admin: Pubkey,
}

pub fn handler(ctx: Context<InitTokenConfig>, ix: InitTokenConfigIx) -> Result<()> {
    let protocol_config = load_protocol_config(&ctx.accounts.protocol_config.to_account_info())?;
    require_keys_eq!(
        protocol_config.admin(),
        ctx.accounts.admin.key(),
        VelaError::UnauthorizedAdmin
    );

    let token_config = &mut ctx.accounts.token_config;
    token_config.mint = ctx.accounts.mint.key();
    token_config.token_program = *ctx.accounts.mint.to_account_info().owner;
    token_config.billing_rail = ix.billing_rail.clone();
    require!(
        ctx.accounts.mint.decimals == ix.decimals,
        VelaError::TokenConfigDecimalsMismatch
    );
    token_config.decimals = ix.decimals;
    token_config.enabled = true;
    token_config.oracle_reference = Pubkey::default();
    token_config.admin = ctx.accounts.admin.key();
    token_config.created_at = Clock::get()?.unix_timestamp;
    token_config.bump = ctx.bumps.token_config;
    token_config.version = CURRENT_ACCOUNT_VERSION;
    token_config._reserved = [0; ACCOUNT_RESERVED_BYTES];

    emit!(TokenConfigCreated {
        mint: token_config.mint,
        billing_rail: ix.billing_rail,
        decimals: ix.decimals,
        admin: token_config.admin,
    });

    Ok(())
}
