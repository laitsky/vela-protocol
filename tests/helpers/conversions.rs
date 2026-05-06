use anchor_lang::prelude::Pubkey;
use solana_address::Address;
use solana_instruction::AccountMeta;

pub fn token_2022_address() -> Address {
    Address::from(spl_token_2022::id().to_bytes())
}

pub fn spl_token_address() -> Address {
    Address::from(spl_token::id().to_bytes())
}

pub fn convert_account_meta(
    meta: anchor_lang::solana_program::instruction::AccountMeta,
) -> AccountMeta {
    if meta.is_writable {
        AccountMeta::new(to_address(meta.pubkey), meta.is_signer)
    } else {
        AccountMeta::new_readonly(to_address(meta.pubkey), meta.is_signer)
    }
}

pub fn to_address(pubkey: Pubkey) -> Address {
    Address::from(pubkey.to_bytes())
}

pub fn to_anchor_pubkey(address: Address) -> Pubkey {
    Pubkey::new_from_array(address.to_bytes())
}
