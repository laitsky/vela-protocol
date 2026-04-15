use anchor_lang::prelude::Pubkey;
use anchor_lang::Discriminator;
use solana_sdk::hash::hash;
use vela_protocol::errors::VelaError;
use vela_protocol::state::mandate::VelaMandate;
use vela_protocol::state::stream_mandate::StreamMandate;

#[test]
fn stream_mandate_pda_seeds() {
    let subscriber = Pubkey::new_unique();
    let merchant = Pubkey::new_unique();
    let index: u64 = 7;
    let (pda, _bump) = Pubkey::find_program_address(
        &[
            StreamMandate::SEED_PREFIX,
            subscriber.as_ref(),
            merchant.as_ref(),
            &index.to_le_bytes(),
        ],
        &vela_protocol::ID,
    );

    let (pda2, _) = Pubkey::find_program_address(
        &[b"stream", subscriber.as_ref(), merchant.as_ref(), &index.to_le_bytes()],
        &vela_protocol::ID,
    );

    assert_eq!(pda, pda2);
    assert_eq!(StreamMandate::SEED_PREFIX, b"stream");
}

#[test]
fn stream_mandate_discriminator() {
    let expected = hash(b"account:StreamMandate").to_bytes();
    let disc = StreamMandate::DISCRIMINATOR;

    assert_eq!(disc.len(), 8);
    assert_eq!(disc.as_ref(), &expected[..8]);
    assert_ne!(disc, VelaMandate::DISCRIMINATOR);
}

#[test]
fn stream_error_band_is_reserved() {
    assert_eq!(VelaError::StreamNotActive as u32, 6700);
    assert_eq!(VelaError::InvalidStreamAddress as u32, 6711);
}
