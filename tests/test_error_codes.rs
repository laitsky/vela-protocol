use anchor_lang::error::Error;
use vela_protocol::{
    errors::VelaError,
    state::{ACCOUNT_RESERVED_BYTES, CURRENT_ACCOUNT_VERSION, LEGACY_ACCOUNT_VERSION},
};

const ANCHOR_TOML: &str = include_str!("../Anchor.toml");
const PROGRAM_CARGO_TOML: &str = include_str!("../programs/vela-protocol/Cargo.toml");

fn error_code(error: VelaError) -> u32 {
    match Error::from(error) {
        Error::AnchorError(anchor_error) => anchor_error.error_code_number,
        other => panic!("expected AnchorError, got {other:?}"),
    }
}

#[test]
fn legacy_error_codes_remain_pinned() {
    assert_eq!(error_code(VelaError::PullTooEarly), 6000);
    assert_eq!(error_code(VelaError::MandateNotActive), 6001);
    assert_eq!(error_code(VelaError::UnauthorizedAdmin), 6015);
    assert_eq!(error_code(VelaError::InvalidFrequency), 6059);
    assert_eq!(error_code(VelaError::UsageReportAlreadySettled), 6061);
}

#[test]
fn phase_40_domain_ranges_are_reserved() {
    assert_eq!(error_code(VelaError::MandateVersionUnsupported), 6100);
    assert_eq!(error_code(VelaError::PlanVersionUnsupported), 6200);
    assert_eq!(error_code(VelaError::MigrationPreconditionFailed), 6300);
    assert_eq!(error_code(VelaError::AgentMandateVersionUnsupported), 6400);
}

#[test]
fn shared_metadata_and_upgradeable_config_are_declared() {
    assert_eq!(LEGACY_ACCOUNT_VERSION, 0);
    assert_eq!(CURRENT_ACCOUNT_VERSION, 1);
    assert_eq!(ACCOUNT_RESERVED_BYTES, 64);

    assert!(
        PROGRAM_CARGO_TOML.contains("version = \"0.2.0\""),
        "expected Cargo.toml to advertise 0.2.0, got:\n{PROGRAM_CARGO_TOML}"
    );
    assert!(
        ANCHOR_TOML.contains("vela_protocol"),
        "expected Anchor.toml to mention vela_protocol"
    );
    assert!(
        ANCHOR_TOML.contains("vela_transfer_hook"),
        "expected Anchor.toml to mention vela_transfer_hook"
    );
    assert!(
        ANCHOR_TOML.contains("[test]") && ANCHOR_TOML.contains("upgradeable = true"),
        "expected Anchor.toml to opt into upgradeable test deployment"
    );
}
