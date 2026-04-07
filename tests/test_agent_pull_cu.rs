#[path = "helpers/mod.rs"]
mod helpers;

use helpers::TestHarness;
use solana_signer::Signer;

/// Reserved for final 300K CU guardrail assertions in 27-05.
#[test]
fn test_agent_pull_cu_scaffold_fixture_plumbing() {
    let mut harness = TestHarness::new();
    let admin = harness.merchant.insecure_clone();
    let (fixture, service, service_wrapped_account) =
        harness.agent_mandate_pull_fixture(&admin, 8_000_000);

    assert_eq!(fixture.authority.pubkey().to_bytes().len(), 32);
    assert_eq!(service.to_bytes().len(), 32);
    assert_eq!(service_wrapped_account.to_bytes().len(), 32);
}
