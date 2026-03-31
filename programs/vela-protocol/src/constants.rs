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
