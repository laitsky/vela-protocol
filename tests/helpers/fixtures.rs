use anchor_lang::prelude::Pubkey;
use solana_keypair::Keypair;

pub struct SubscriptionFixture {
    pub subscriber: Keypair,
    pub plan: Pubkey,
    pub mandate: Pubkey,
    pub credential_mint: Pubkey,
    pub usdc_mint: Pubkey,
    pub wrapped_usdc_mint: Pubkey,
    pub wrapping_vault: Pubkey,
    pub subscriber_token_account: Pubkey,
    pub merchant_token_account: Pubkey,
}

pub struct AgentMandateFixture {
    pub authority: Keypair,
    pub agent: Keypair,
    pub spl_usdc_mint: Pubkey,
    pub wrapped_usdc_mint: Pubkey,
    pub wrapping_vault: Pubkey,
    pub extra_account_meta_list: Pubkey,
    pub token_config: Pubkey,
    pub authority_spl_usdc_ata: Pubkey,
    pub agent_mandate: Pubkey,
    pub agent_mandate_wrapped_ata: Pubkey,
}
