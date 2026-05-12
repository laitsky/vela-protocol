# Vela Protocol Post-Hardening Security Audit

Audit date: 2026-05-11  
Scope: current local working tree across `vela-protocol`, synced `../vela-sdk`, devnet demo state, and Velapay consumer compatibility checks.  
Auditor: Codex local audit pass.

## Executive Summary

The previously reported Critical and High protocol findings have been patched and covered by regression tests. The devnet programs were rebuilt and upgraded after this audit pass found a local/deployed artifact mismatch, and deployed bytecode plus on-chain IDLs now match the current local artifacts.

Production security posture for the patched scope is green for devnet consumption:

- Settlement recipient validation is enforced before pull, stream, pause, cancel, rate-update, and positive-proration settlement CPIs.
- `init_config` is bound to the upgrade authority through Program/ProgramData validation.
- `demo_approve_pull` is removed from the production program interface and absent from generated IDLs.
- `PullApproval` is period-bound and short-lived.
- Usage reports are merchant-only, period-bound, closed-period-only, and marked settled only after payment succeeds.
- Usage computation uses encrypted usage units from the report and plaintext pricing terms from the on-chain `UsagePlan`.
- Tiered pricing uses cumulative tier boundaries with final-tier unlimited semantics.
- `update_plan`, `update_mandate`, positive proration, stream min-settle interval, and shared settlement invariants now have regression coverage.

No new Critical or High vulnerabilities were found in this pass.

## Fixed Critical / High Findings

| Prior finding | Status | Evidence |
| --- | --- | --- |
| Settlement redirection to arbitrary token accounts | Fixed | `validate_token_2022_transfer_accounts` plus pull/stream destination-owner tests |
| Streaming settlement mint/destination not bound | Fixed | Shared stream settlement helper validates mint, source authority, and merchant destination owner |
| Production `demo_approve_pull` bypass | Fixed | Source handler removed; IDL/SDK checks confirm no `demoApprovePull` |
| Usage ciphertext not bound to on-chain plan terms | Fixed | Report stores one encrypted usage-units field and a protocol-computed terms hash; computation reads plan terms on-chain |
| Tiered pricing cumulative/unlimited bugs | Fixed | Circuit implementation and reference/proptest vectors pass |
| First-caller `init_config` takeover | Fixed | `init_config` validates upgradeable loader ProgramData authority |
| Approval timing / stale-period approval use | Fixed | `PullApproval` has `period_start`, `period_end`, `valid_until`; hook and protocol checks reject stale/wrong-period approvals |
| Usage report marked settled before payment | Fixed | Callback leaves report unpaid; `execute_pull` marks settled after successful settlement |

## Current Residual Risks

### Medium - Arcium Devnet Callback Delivery Is Externally Blocked

Devnet demo smoke successfully submits `request_validation` through the real Arcium path, and the Arcium computation account reaches `finalized`, but the callback transaction is not delivered. Recent Arcium follow-up transactions against the computation fail inside the Arcium program with `InvalidAuthority`, leaving `callbackTransactionsSubmittedBm = 0`.

Impact: The demo correctly fails closed instead of using a bypass, but the recurring/usage demo cannot complete end-to-end until Arcium devnet callback execution is healthy.

Action: Coordinate with Arcium or use a known-good Arcium devnet executor/cluster. Do not reintroduce an on-chain bypass.

### Medium - JavaScript Advisory Noise Remains

`bun audit` still reports transitive Solana dependency advisories:

- `bigint-buffer <=1.1.5` via `@solana/spl-token`
- `uuid >=11.0.0 <11.1.1` via `@solana/web3.js` / `rpc-websockets`

Action: Track upstream Solana dependency releases or test a broader dependency update before mainnet. This is not caused by this patch, but it should be part of mainnet readiness.

### Low - Local `cargo-audit` Tooling Is Broken

`cargo audit` cannot start locally because the installed binary links `/opt/homebrew/opt/openssl@1.1/lib/libssl.1.1.dylib`.

Action: Reinstall `cargo-audit` against the current OpenSSL or run Rust advisory scanning in CI/container before mainnet.

### Low - Local Ignored Key Material Exists

Devnet program keypairs are present under `target/deploy` and ignored by git. That is expected for the local rollout scripts but should not be included in support bundles or artifacts.

Action: Keep these files local/ignored; use secure storage for production authorities.

## Verification Log

| Check | Result | Notes |
| --- | --- | --- |
| `cargo fmt --check` | PASS | No formatting drift |
| `cargo clippy --workspace --all-targets` | PASS | No clippy errors |
| `cargo test -p encrypted-ixs -- --nocapture` | PASS | 9 tests; generated negative tests print expected caught overflow panics |
| `cargo test -p vela-transfer-hook -- --nocapture` | PASS | 2 tests |
| `cargo test -p vela-protocol -- --nocapture` | PASS | Full Rust suite passed |
| `NO_DNA=1 anchor build` | PASS | Known Arcium generated IDL stack warning appears; final deployable SBF logs are separately verified |
| `bun run typecheck` | PASS | Protocol TS checks |
| `bun test ts-tests` | PASS | 4 TS tests |
| `bun run verify:sbf` | PASS | Final deployable SBF logs contain no blocked stack-frame warnings |
| `bun run program-ids:check` | PASS | Manifest and source IDs in sync |
| `ARCIUM_UPLOAD_CHUNK_SIZE=1 bun run arcium:comp-defs:devnet` | PASS | All four comp-defs finalized |
| `bash scripts/verify-rollout.sh` | PASS | SDK, webhook, dashboard, admin, portal, checkout, widget, demo, synthetic, docs checks passed |
| `NO_DNA=1 bash scripts/upgrade-devnet-safe.sh` | PASS | Rebuilt, rechecked consumers, upgraded devnet, verified hashes and on-chain IDLs |
| `bash scripts/assert-deployed-hash.sh` | PASS | Included in safe upgrade final report |
| `bun audit` | FAIL | Known transitive advisories listed above |
| `cargo audit` | BLOCKED | Local OpenSSL 1.1 dynamic library issue |
| Secret pattern scan | REVIEWED | Matches were env names, test tokens, docs placeholders, or local ignored key references; no source secret found |

## Devnet Rollout State

Current devnet program IDs are unchanged:

- `vela_protocol`: `CVM6UqbwKgHckZzm8R2qbN3BWhCTdk1PsSeEQLchkwKT`
- `vela_transfer_hook`: `3agVoFp4NZFuKbVqCV8HbjSZn1xW4Utk4U1Wir3TKjZ9`

Latest audited upgrade signatures:

- `vela_transfer_hook`: `31pxzopX18NtDc8rQGd8V8aBb4dHe9t7dSH2e1EK1q2qQY2Yp6ve5HA2vN3waPQFY9YgD5RQoPXQkTrztuMQ8yor`
- `vela_protocol`: `352TDajzYHoaMubRjm8jVgUq4HhHjKGtqpB3rjm8YbmT1eFrBoNb6ieeARKjyR4bEXf4vNJncTAfgrCcMgX5kE8X`

All Arcium computation definitions are present and finalized:

- `validate_mandate`
- `usage_charge`
- `tiered_pricing`
- `record_billing_event`

## Recommendation

This working tree is ready to commit for the hardened devnet/SDK compatibility release, subject to the known Arcium devnet callback liveness issue and the non-blocking supply-chain/tooling items above. Do not publish the SDK until the commit is made and the package contents are inspected with the final built `dist/`.
