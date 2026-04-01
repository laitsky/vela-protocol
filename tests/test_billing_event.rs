#[path = "helpers/mod.rs"]
mod helpers;

use helpers::TestHarness;
use solana_signer::Signer;
use vela_protocol::{
    constants::MIN_FREQUENCY_SECONDS,
    state::{BillingEvent, VelaPlan},
};

fn setup_fixture() -> (TestHarness, helpers::SubscriptionFixture, VelaPlan) {
    let mut harness = TestHarness::new();
    let fixture = harness.subscribe_fixture(25_000_000, MIN_FREQUENCY_SECONDS, 0, 2);
    let plan: VelaPlan = harness.fetch_anchor_account(&fixture.plan);
    (harness, fixture, plan)
}

#[test]
fn test_billing_event_created() {
    let (mut harness, fixture, plan) = setup_fixture();
    let encrypted_blob = [[7u8; 32]; 10];
    let nonce = 99u128;

    let billing_event = harness.create_billing_event(
        &fixture.mandate,
        &harness.merchant_pubkey(),
        &anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes()),
        plan.plan_id,
        1,
        encrypted_blob,
        nonce,
    );

    let stored: BillingEvent = harness.fetch_anchor_account(&billing_event);
    assert_eq!(stored.mandate, fixture.mandate);
    assert_eq!(stored.merchant, harness.merchant_pubkey());
    assert_eq!(
        stored.subscriber,
        anchor_lang::prelude::Pubkey::new_from_array(fixture.subscriber.pubkey().to_bytes())
    );
    assert_eq!(stored.plan_id, plan.plan_id);
    assert_eq!(stored.encrypted_blob, encrypted_blob);
    assert_eq!(stored.nonce, nonce);
    assert_eq!(stored.created_at, harness.current_timestamp());
}

#[test]
fn test_billing_event_unique_per_pull() {
    let (harness, fixture, _) = setup_fixture();

    let first = harness.derive_billing_event_address(&fixture.mandate, 1);
    let second = harness.derive_billing_event_address(&fixture.mandate, 2);

    assert_ne!(first, second, "billing event PDAs must be unique per pull");
}

#[test]
fn test_billing_event_immutable() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should resolve");
    let instructions_dir = repo_root.join("programs/vela-protocol/src/instructions");
    let billing_callback = instructions_dir.join("billing_callback.rs");
    let callback_source =
        std::fs::read_to_string(&billing_callback).expect("billing callback source should exist");

    assert!(
        !callback_source.contains("close ="),
        "billing events must never expose a close authority",
    );

    for entry in std::fs::read_dir(&instructions_dir).expect("instructions dir should exist") {
        let entry = entry.expect("dir entry should read");
        if entry.file_name() == "billing_callback.rs"
            || entry.file_name() == "request_billing_record.rs"
        {
            continue;
        }
        let source = std::fs::read_to_string(entry.path()).expect("instruction source should read");
        assert!(
            !source.contains("Account<'info, BillingEvent>"),
            "BillingEvent should not be mutable outside the billing request/callback flow, found in {}",
            entry.path().display()
        );
    }
}
