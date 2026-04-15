use anchor_lang::prelude::*;

use super::CURRENT_ACCOUNT_VERSION;

pub const STREAM_PROTO_RESERVED_BYTES: usize = 56;

#[account]
pub struct StreamMandateProto {
    pub version: u8,
    pub subscriber: Pubkey,
    pub merchant: Pubkey,
    pub mint: Pubkey,
    pub rate_per_second: u64,
    pub authorized_max_rate: u64,
    pub last_settled_ts: i64,
    pub total_streamed: u64,
    pub max_streamed: Option<u64>,
    pub paused_at: Option<i64>,
    pub min_settle_interval: u32,
    pub status: StreamStatusProto,
    pub mandate_index: u64,
    pub bump: u8,
    pub _reserved: [u8; STREAM_PROTO_RESERVED_BYTES],
}

impl StreamMandateProto {
    pub const SEED_PREFIX: &'static [u8] = b"stream";
    pub const SIZE: usize = 8 + 1 + 32 + 32 + 32 + 8 + 8 + 8 + 8 + 9 + 9 + 4 + 1 + 8 + 1 + 56;

    pub fn current_version() -> u8 {
        CURRENT_ACCOUNT_VERSION
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub enum StreamStatusProto {
    Active,
    Paused,
    Cancelled,
}
