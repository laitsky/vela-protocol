<!-- markdownlint-disable MD013 -->

# vela-protocol

Private, programmable payment authority on Solana. Subscriptions, per-second streams, usage metering, and agent budgets - all on one authorization primitive, with mandate validation and charge computation running on encrypted data via Arcium MPC.

## Overview

vela-protocol is a dual-program Anchor workspace:

| Program | ID | Purpose |
| --- | --- | --- |
| `vela-protocol` | `CVM6UqbwKgHckZzm8R2qbN3BWhCTdk1PsSeEQLchkwKT` | Core billing logic: plans, mandates, Arcium callbacks |
| `vela-transfer-hook` | `3agVoFp4NZFuKbVqCV8HbjSZn1xW4Utk4U1Wir3TKjZ9` | Token-2022 transfer hook: enforces PullApproval on every transfer |

Billing logic runs on ciphertexts via four Arcium MPC circuits (`encrypted-ixs/`). The programs never see sensitive billing values in plaintext; Arcium returns only the approval flag and the charge amount.

The protocol supports merchant-selected Token-2022 billing mints through on-chain `TokenConfig` accounts. Wrapped USDC is the compatibility default, while enabled Token-2022 mints such as PYUSD or EURC can be bound directly to periodic plans and stream mandates.

## Payment Flow

```text
Merchant creates a plan with billing_mint + TokenConfig
                                        |
Subscriber creates a mandate for that plan
                                        |
Subscriber funds the mandate-owned Token-2022 account
  - wrapped USDC plans wrap SPL USDC first
  - PYUSD/EURC-style plans use the selected Token-2022 mint directly
                                        |
Merchant submits encrypted usage report (usage plans only)
                                        |
Keeper calls request_validation --------> Arcium validate_mandate circuit
                                             (encrypted balance, timing, limits)
                                        |
Arcium calls validate_mandate_callback -> PullApproval stored on-chain
                                        |
Keeper calls execute_pull -------------> Transfer hook checks PullApproval
                                             - approved + amount <= approved_amount
                                             - not expired
                                             - billing mint matches TokenConfig
                                        |
Keeper calls request_billing_record ---> Arcium record_billing_event circuit
                                        |
Arcium calls record_billing_event_callback -> BillingEvent stored (immutable)
```

**Fail-closed (D-01):** `execute_pull` is blocked if no valid `PullApproval` exists. Arcium unavailability rejects the pull.

## Programs

### Main Program

#### Plan Management

| Instruction | Description |
| --- | --- |
| `create_plan` | Create a flat-rate subscription plan for an enabled billing mint; mints a Token-2022 credential mint |
| `create_usage_plan` | Create a usage-based plan with 1–5 pricing tiers and a per-period cap |
| `subscribe` | Subscriber creates a mandate and receives a non-transferable credential NFT |
| `cancel` | Subscriber cancels their mandate |

#### Billing Execution

| Instruction | Description |
| --- | --- |
| `request_validation` | Submit encrypted mandate inputs to Arcium for eligibility check; requires writable request-state PDA |
| `validate_mandate_callback` | Arcium callback: validates request-state PDA and stores `PullApproval` with approval flag and amount |
| `request_usage_computation` | Queue the ciphertext committed in `UsageReport` to Arcium for charge calculation; requires writable request-state PDA |
| `usage_charge_callback` | Arcium callback: validates request-state PDA and stores usage-computed charge in `PullApproval` |
| `execute_pull` | Transfer hook validates `PullApproval`, executes the billing-token transfer |
| `request_billing_record` | Submit billing inputs to Arcium for encrypted audit record; requires writable request-state PDA |
| `record_billing_event_callback` | Arcium callback: validates request-state PDA and stores immutable `BillingEvent` |

#### Token Configuration and Wrapping

| Instruction | Description |
| --- | --- |
| `init_token_config` | Register a billable Token-2022 mint and billing rail |
| `update_token_config` | Enable, disable, or update a registered mint |
| `init_wrapped_mint` | Create Token-2022 wrapped USDC mint with transfer hook extension |
| `wrap` | Deposit SPL USDC, receive wrapped USDC 1:1 |
| `unwrap` | Burn wrapped USDC, receive SPL USDC from vault (bypasses hook via vault check) |

#### Configuration

| Instruction | Description |
| --- | --- |
| `init_config` | Set Arcium cluster reference (Cerberus devnet / Manticore mainnet) |
| `update_config` | Update cluster configuration |
| `init_keeper_config` | Configure keeper authority and mode (Centralized / TukTuk) |
| `update_keeper_config` | Update keeper settings |
| `init_validate_mandate_comp_def` | Register Arcium computation definition for mandate validation |
| `init_usage_charge_comp_def` | Register Arcium computation definition for single-tier usage pricing |
| `init_tiered_pricing_comp_def` | Register Arcium computation definition for tiered usage pricing |
| `init_record_billing_comp_def` | Register Arcium computation definition for billing event |

### vela-transfer-hook

| Instruction | Description |
| --- | --- |
| `init_extra_account_meta_list` | Register extra accounts required by the transfer hook |
| `transfer_hook` | Validate transfer: checks `PullApproval` exists, is approved, is unexpired, and amount ≤ `approved_amount` |

## Account Reference

| Account | Seeds | Description |
| --- | --- | --- |
| `ProtocolConfig` | `[b"config"]` | Singleton - admin, Arcium cluster, wrapped mint refs |
| `KeeperConfig` | `[b"keeper-config"]` | Keeper authority, endpoint, mode |
| `MerchantState` | `[b"merchant", merchant]` | Per-merchant plan counter |
| `TokenConfig` | `[b"token_config", mint]` | Enabled billing mint configuration and rail |
| `VelaPlan` | `[b"plan", merchant, plan_id]` | Flat-rate subscription template with billing mint |
| `UsagePlan` | `[b"usage_plan", merchant, plan_id]` | Usage-based pricing template (up to 5 tiers) |
| `VelaMandate` | `[b"mandate", subscriber, plan]` | Active subscription: tracks pull count, next due, billing type |
| `PullApproval` | `[b"approval", mandate]` | Per-period approval from Arcium; `valid_until` is a short approval expiry |
| `UsageReport` | `[b"usage_report", mandate, period_start]` | Encrypted usage data submitted by merchant |
| `BillingEvent` | `[b"billing", mandate, pulls_executed]` | Encrypted audit record, no close authority |

## Arcium Circuits

Circuits are defined in `encrypted-ixs/src/lib.rs` and compiled to `build/`.

| Circuit | Encrypted Inputs | Output |
| --- | --- | --- |
| `validate_mandate` | amount, balance, timestamp, next_due, expiry, pull count | `bool` (approved) |
| `usage_charge` | usage units, rate per unit, max charge | `u64` (capped charge) |
| `tiered_pricing` | usage units, 5 tier boundaries, 5 tier rates, tier count, cap | `u64` (capped charge) |
| `record_billing_event` | amount, timestamp, pulls, period start/end, payment method | MXE-encrypted `[u64; 8]` |

All circuits are written to satisfy MPC determinism constraints: no early returns, fixed-size loops, and conditional assignment instead of branching.

**Build artifacts in `build/` (gitignored; regenerated via `arcium build`):**

- `*.arcis` - compiled circuit binary
- `*.arcis.ir` - intermediate representation
- `*.idarc` - Arcium circuit metadata
- `*.ts` - generated TypeScript type definitions
- `*.hash` / `*.weight` - content hash and resource estimate

Run `arcium build` before `anchor build` on a fresh clone. The program embeds `.arcis` bytecode at compile time via `include_bytes!`.

## Development

### Prerequisites

| Tool | Version |
| --- | --- |
| Rust | 1.89.0 (pinned in `rust-toolchain.toml`) |
| Solana CLI (Agave) | 2.3.0+ |
| Anchor CLI | 0.32.1 |
| Bun | 1.3.11+ |

```sh
# Install Agave CLI
agave-install init 2.3.0

# Install Anchor CLI
cargo install --git https://github.com/coral-xyz/anchor --tag v0.32.1 anchor-cli

# Install JS dependencies
bun install
```

### Build

```sh
# Everyday local build for anyone cloning this repo.
# Validates repo-local program ids, then runs Arcium + Anchor.
bun run build:local

# One-time setup: save the current devnet program keypairs somewhere persistent
#
# Default location:
#   ~/.config/velapay/keys/devnet/
#
# This prevents future `anchor build` runs from silently generating new
# keypairs if `target/` gets deleted.
# Keep these files private and outside git.
bun run keys:backup:devnet

# Safe daily build for the real devnet program ids.
# Restores the saved keypairs into target/deploy, then builds both programs
# with the devnet-compatible SBF architecture.
bun run build:devnet-safe

# Compile Arcium circuits to build/ (required before `anchor build` on a fresh clone)
arcium build

# Build Anchor programs
anchor build

# Or run both steps together
bun run build:programs

# Type-check TypeScript
bun run typecheck
```

If your saved keypairs live somewhere else, override the default location:

```sh
VELA_PROGRAM_KEY_DIR=/secure/path/to/devnet-keys bun run build:devnet-safe
```

Use `bun run build:local` for normal standalone repo builds. Use `bun run build:devnet-safe` only when you want to preserve the real devnet program identities with the saved keypairs.

The devnet-safe build path intentionally uses `cargo-build-sbf` directly instead of raw `anchor build` for deployable artifacts. This avoids accidental program-id churn and lets each program use the SBF architecture currently accepted by devnet.

### Test

```sh
# Rust integration tests (LiteSVM, in-process)
cargo test -p vela-protocol -- --nocapture

# TypeScript integration tests (Bun + LiteSVM)
bun test ts-tests/

# Full CI pipeline (rebuilds Arcium circuits first)
bun run ci:protocol
```

### Safe Upgrade and Deployment

```sh
# Full devnet upgrade flow for the real deployed programs.
# This builds, verifies downstream compatibility, upgrades both programs,
# uploads IDLs, then proves the deployed bytes and IDLs match local artifacts.
bun run upgrade:devnet

# Alias for the same safe upgrade flow.
bun run deploy:devnet

# Run the compatibility gate without deploying.
bun run verify:rollout

# Verify the currently deployed devnet bytecode and on-chain IDLs
# match target/deploy and target/idl.
bun run verify:deployed

# Verify the saved SBF build logs do not contain blocked loader warnings.
bun run verify:sbf

# Same as verify:rollout.
bun run compat:quick

# Initialize protocol config after deploy
# (see ts-tests/setup.ts for account initialization order)
```

Avoid raw `anchor deploy` for devnet. Use the guarded script path so deployed program ids, IDLs, upgrade authority, and consumer checks stay aligned with `config/program-ids.json`.

The upgrade command performs these gates:

| Gate | What it protects |
| --- | --- |
| Program ID check | Prevents deploying a binary with stale `declare_id!()` values |
| Keypair restore | Ensures `target/deploy/*-keypair.json` matches the configured devnet program ids |
| IDL sync | Ensures `target/idl` matches the SDK IDL copies |
| Upgrade authority check | Ensures the configured wallet can upgrade the existing programs |
| SBF stack-warning gate | Blocks final deployable artifacts with Solana loader stack-frame warnings |
| SDK checks | Catches instruction-builder, account-deserializer, and event-type drift |
| Webhook checks | Catches public event schema and fixture drift |
| Dashboard worker checks | Catches indexer, queue, and fanout drift |
| Checkout/widget checks | Catches hosted checkout and embeddable subscribe flow drift |
| Byte hash check | Dumps devnet programs and compares them with local `.so` artifacts |
| On-chain IDL check | Fetches devnet IDLs and compares them with local IDLs |

For emergency diagnostics, consumer checks can be skipped:

```sh
SKIP_CONSUMER_CHECKS=1 bun run verify:rollout
```

Do not use this override for normal releases. It is intended only for isolating failures while developing the upgrade pipeline.

SBF stack-frame warnings in final deployable program logs are blocked by
default. For local diagnostics only, you can continue past them with:

```sh
ALLOW_SBF_STACK_WARNINGS=1 bun run build:devnet-safe
```

The release path also requires build logs under `target/sbf-build-logs`.
For diagnostics against older artifacts only, set `REQUIRE_SBF_BUILD_LOGS=0`.

Do not use this override for public releases or mainnet. A stack-frame warning
in a final deployable artifact must be fixed upstream or fixed in an audited
local patch.

`arcium build` may emit a known stack-frame warning for Arcium's generated
IDL helper `arcium_client::idl::arcium::utils::Account::try_from`. That command
still exits successfully and produces deployable `.so` artifacts, while the
direct final `cargo-build-sbf` logs for `vela_protocol` and `vela_transfer_hook`
remain clean. The rollout gate allows only that exact warning in
`target/checked-build-logs/arcium-build*.log`; any other checked-log stack
warning, and any final artifact stack warning, still fails verification.

## Protocol Compatibility Policy

Program upgrades keep the same Solana program address, but they can still break downstream consumers if the public interface changes. Treat these as compatibility-sensitive changes:

| Protocol change | Repos that usually need review |
| --- | --- |
| Instruction accounts or args changed | `vela-sdk`, callers that build transactions |
| Account layout changed | `vela-sdk`, `vela-dashboard`, indexers, admin tools |
| Event shape or encoding changed | `vela-sdk/events`, `vela-dashboard` worker, `vela-webhook`, docs |
| Program ids changed | Every repo with generated program ids |
| Plan, subscribe, or checkout semantics changed | `vela-sdk`, `vela-dashboard`, `vela-checkout`, `vela-widget`, demos |
| Pull or settlement semantics changed | `vela-sdk`, keepers, dashboard worker, webhook consumers |
| Token semantics changed | Dashboard, checkout, widget, portal, docs |

The safe upgrade scripts verify that the known downstream repos still pass their targeted checks. They do not decide how to migrate consumers for you; when an IDL or event contract changes, update the affected repos first, then run `bun run verify:rollout`, then upgrade.

## Program Identity and Keypairs

Solana program addresses are determined by the program keypairs in `target/deploy`, not by the deployer wallet. The deployer wallet is the upgrade authority; it does not define the program address.

Persistent devnet program keypairs are expected at:

```text
~/.config/velapay/keys/devnet/vela_protocol-keypair.json
~/.config/velapay/keys/devnet/vela_transfer_hook-keypair.json
```

Back them up securely and never commit them. On a new machine, restore those files and run:

```sh
bun run keys:prepare
solana address -k target/deploy/vela_protocol-keypair.json
solana address -k target/deploy/vela_transfer_hook-keypair.json
```

The addresses must match `config/program-ids.json` before any deploy or upgrade.

## Program Architecture Notes

- **Credential NFTs** - Non-transferable Token-2022 mints issued per mandate, proving active subscription.
- **Billing type routing** - `VelaMandate.billing_type` is `Flat` or `Usage`. Usage mandates require a settled `UsageReport` before `request_validation` proceeds to `execute_pull`.
- **Usage pricing envelope** - Usage plans support up to 5 tiers. Create/update enforces protocol bounds on rates, tier boundaries, and per-period caps so encrypted `u64` pricing intermediates stay inside the overflow-safe envelope.
- **Usage ciphertext commitment** - Merchants commit the exact usage-computation ciphertext in `submit_usage_report`; `request_usage_computation` queues only the stored report payload, preventing a later caller from swapping usage or pricing inputs.
- **Arcium request state** - Validation, usage-computation, and billing-record requests use dedicated request-state PDAs keyed by flow, mandate, and business subject. Callbacks validate the stored request offset and complete that PDA, preventing duplicate in-flight requests from stranding callbacks.
- **Keeper modes** - `Centralized` uses an off-chain keeper polling subscriptions. `TukTuk` routes through the on-chain task queue (not yet live).
- **Minimum frequency** - Plans must have `frequency >= 3600` seconds (1 hour).
- **Pending billing guard** - `execute_pull` is blocked if the previous pull has no finalized `BillingEvent`, enforcing sequential billing records.

## Repo Layout

```text
programs/
  vela-protocol/       # Main billing program
  vela-transfer-hook/  # Token-2022 transfer hook validator
encrypted-ixs/          # Arcium circuit definitions (arcis DSL)
build/                  # Compiled circuit artifacts (gitignored)
tests/                  # Rust integration tests (13 tests, LiteSVM)
ts-tests/               # TypeScript integration tests (5 tests, Bun)
migrations/             # Anchor deploy migrations
```
