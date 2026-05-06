use anchor_lang::prelude::Pubkey;
use vela_protocol::state::{TokenConfig, VelaMandate};

pub fn derive_mandate_v2_pda(
    subscriber: &Pubkey,
    merchant: &Pubkey,
    mandate_index: u64,
    program_id: &Pubkey,
) -> (Pubkey, u8) {
    let mandate_index_bytes = mandate_index.to_le_bytes();
    Pubkey::find_program_address(
        &[
            VelaMandate::SEED_PREFIX,
            subscriber.as_ref(),
            merchant.as_ref(),
            mandate_index_bytes.as_ref(),
        ],
        program_id,
    )
}

pub fn derive_token_config_pda(mint: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[TokenConfig::SEED_PREFIX, mint.as_ref()], program_id)
}
