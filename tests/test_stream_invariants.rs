use std::{fs, path::PathBuf};

const STREAMING_SOURCES: &[&str] = &[
    "programs/vela-protocol/src/instructions/create_stream_mandate.rs",
    "programs/vela-protocol/src/instructions/execute_stream.rs",
    "programs/vela-protocol/src/instructions/pause_stream.rs",
    "programs/vela-protocol/src/instructions/resume_stream.rs",
    "programs/vela-protocol/src/instructions/cancel_stream.rs",
    "programs/vela-protocol/src/instructions/update_stream_rate.rs",
    "programs/vela-protocol/src/instructions/stream_account.rs",
    "programs/vela-protocol/src/state/stream_mandate.rs",
    "programs/vela-transfer-hook/src/lib.rs",
];

const FORBIDDEN_PATTERNS: &[&str] = &["Clock::slot", "clock.slot", ".slot_history"];
const CLOCK_HELPER_ONLY_SOURCES: &[&str] = &[
    "programs/vela-protocol/src/instructions/stream_account.rs",
    "programs/vela-protocol/src/state/stream_mandate.rs",
];

#[test]
fn no_slot_clock_reads() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut failures = Vec::new();

    for rel in STREAMING_SOURCES {
        let path = workspace.join(rel);
        let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {}", rel, e));
        for pat in FORBIDDEN_PATTERNS {
            for (line_no, line) in src.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    continue;
                }
                if line.contains(pat) {
                    failures.push(format!("{}:{}: {}", rel, line_no + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "slot-clock reads found:\n{}",
        failures.join("\n")
    );
}

#[test]
fn streaming_uses_unix_timestamp_exclusively() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut missing = Vec::new();

    for rel in STREAMING_SOURCES {
        if CLOCK_HELPER_ONLY_SOURCES.contains(rel) {
            continue;
        }

        let src = fs::read_to_string(workspace.join(rel))
            .unwrap_or_else(|e| panic!("read {}: {}", rel, e));
        if !src.contains("unix_timestamp") {
            missing.push(*rel);
        }
    }

    assert!(
        missing.is_empty(),
        "streaming sources missing unix_timestamp reads: {:?}",
        missing
    );
}
