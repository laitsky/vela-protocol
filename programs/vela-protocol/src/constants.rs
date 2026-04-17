use anchor_lang::prelude::Pubkey;

pub const MIN_FREQUENCY_SECONDS: u64 = 3600;
pub const USDC_DECIMALS: u8 = 6;
pub const CREDENTIAL_DECIMALS: u8 = 0;
pub const CREDENTIAL_METADATA_SPACE: usize = 250;

// Arcium computation definition offsets
// These must match the function names in encrypted-ixs/src/lib.rs
// Note: comp_def_offset is evaluated at compile time via const evaluation
// The actual offset value is assigned at build time by Arcium tooling
// Using a placeholder constant here that will be resolved by arcium build
pub const COMP_DEF_OFFSET_VALIDATE_MANDATE: u32 = 0u32;
pub const COMP_DEF_OFFSET_RECORD_BILLING: u32 = 0u32;
// Devnet cluster offset (D-12: stored in ProtocolConfig, this is the default)
pub const DEFAULT_CLUSTER_OFFSET: u64 = 456;

// Transfer hook infrastructure seeds and metadata
pub const MINT_AUTHORITY_SEED: &[u8] = b"mint-authority";
pub const EXTRA_ACCOUNT_METAS_SEED: &[u8] = b"extra-account-metas";
pub const TOKEN_CONFIG_SEED: &[u8] = b"token_config";
pub const WRAPPED_USDC_NAME: &str = "Vela Wrapped USDC";
pub const WRAPPED_USDC_SYMBOL: &str = "sUSDC";
pub const WRAPPED_USDC_URI: &str = "";
pub const TRANSFER_FEE_BASIS_POINTS: u16 = 0;
pub const TRANSFER_FEE_MAXIMUM: u64 = 0;
pub const AGENT_MANDATE_SEED: &[u8] = b"agent-mandate";
pub const AGENT_MANDATE_MAX_SERVICES: usize = 8;
pub const AGENT_PULL_APPROVAL_TTL_SECONDS: i64 = 60;
pub const AGENT_DAILY_RESET_WINDOW_SECONDS: i64 = 86_400;
pub const TRANSFER_HOOK_PROGRAM_ID_BYTES: [u8; 32] = [
    119, 151, 22, 63, 246, 128, 87, 195, 239, 20, 74, 86, 88, 37, 93, 174, 138, 40, 6, 219, 108,
    145, 80, 83, 110, 110, 168, 255, 120, 185, 194, 182,
];
pub const TRANSFER_HOOK_PROGRAM_ID: Pubkey = Pubkey::new_from_array(TRANSFER_HOOK_PROGRAM_ID_BYTES);
