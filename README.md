# vela-protocol

Encrypted recurring billing primitive on Solana. Flat-rate subscriptions and usage-based pricing, with mandate validation and charge computation running on encrypted data via Arcium MPC.

## Overview

vela-protocol is a dual-program Anchor workspace:

| Program | ID | Purpose |
|---|---|---|
| `vela-protocol` | `CVM6UqbwKgHckZzm8R2qbN3BWhCTdk1PsSeEQLchkwKT` | Core billing logic — plans, mandates, Arcium callbacks |
| `vela-transfer-hook` | `3agVoFp4NZFuKbVqCV8HbjSZn1xW4Utk4U1Wir3TKjZ9` | Token-2022 transfer hook — enforces PullApproval on every transfer |

Billing logic runs on ciphertexts via four Arcium MPC circuits (`encrypted-ixs/`). The programs never see sensitive billing values in plaintext — Arcium returns only the approval flag and the charge amount.

## Payment Flow

```
Subscriber wraps SPL USDC → wrapped USDC (Token-2022 w/ transfer hook)
                                        │
Merchant submits encrypted usage report (usage plans only)
                                        │
Keeper calls request_validation ────────► Arcium validate_mandate circuit
                                              (encrypted balance, timing, limits)
                                        │
Arcium calls validate_mandate_callback ─► PullApproval stored on-chain
                                        │
Keeper calls execute_pull ─────────────► Transfer hook checks PullApproval
                                              └─ approved + amount ≤ approved_amount
                                              └─ not expired
                                        │
Keeper calls request_billing_record ───► Arcium record_billing_event circuit
                                        │
Arcium calls record_billing_event_callback ─► BillingEvent stored (immutable)
```

**Fail-closed (D-01):** `execute_pull` is blocked if no valid `PullApproval` exists. Arcium unavailability rejects the pull.

## Programs

### vela-protocol

**Plan management**

| Instruction | Description |
|---|---|
| `create_plan` | Create a flat-rate subscription plan; mints a Token-2022 credential mint |
| `create_usage_plan` | Create a usage-based plan with 1–5 pricing tiers and a per-period cap |
| `subscribe` | Subscriber creates a mandate and receives a non-transferable credential NFT |
| `cancel` | Subscriber cancels their mandate |

**Billing execution**

| Instruction | Description |
|---|---|
| `request_validation` | Submit encrypted mandate inputs to Arcium for eligibility check |
| `validate_mandate_callback` | Arcium callback — stores `PullApproval` with approval flag and amount |
| `request_usage_computation` | Submit encrypted usage report to Arcium for charge calculation |
| `usage_charge_callback` | Arcium callback — stores usage-computed charge in `PullApproval` |
| `execute_pull` | Transfer hook validates `PullApproval`, executes wrapped USDC transfer |
| `request_billing_record` | Submit billing inputs to Arcium for encrypted audit record |
| `record_billing_event_callback` | Arcium callback — stores immutable `BillingEvent` |

**Wrapped USDC**

| Instruction | Description |
|---|---|
| `init_wrapped_mint` | Create Token-2022 wrapped USDC mint with transfer hook extension |
| `wrap` | Deposit SPL USDC → receive wrapped USDC 1:1 |
| `unwrap` | Burn wrapped USDC → receive SPL USDC from vault (bypasses hook via vault check) |

**Configuration**

| Instruction | Description |
|---|---|
| `init_config` | Set Arcium cluster reference (Cerberus devnet / Manticore mainnet) |
| `update_config` | Update cluster configuration |
| `init_keeper_config` | Configure keeper authority and mode (Centralized / TukTuk) |
| `update_keeper_config` | Update keeper settings |
| `init_validate_mandate_comp_def` | Register Arcium computation definition for mandate validation |
| `init_record_billing_comp_def` | Register Arcium computation definition for billing event |

### vela-transfer-hook

| Instruction | Description |
|---|---|
| `init_extra_account_meta_list` | Register extra accounts required by the transfer hook |
| `transfer_hook` | Validate transfer: checks `PullApproval` exists, is approved, is unexpired, and amount ≤ `approved_amount` |

## Account Reference

| Account | Seeds | Description |
|---|---|---|
| `ProtocolConfig` | `[b"config"]` | Singleton — admin, Arcium cluster, wrapped mint refs |
| `KeeperConfig` | `[b"keeper-config"]` | Keeper authority, endpoint, mode |
| `MerchantState` | `[b"merchant", merchant]` | Per-merchant plan counter |
| `VelaPlan` | `[b"plan", merchant, plan_id]` | Flat-rate subscription template |
| `UsagePlan` | `[b"usage_plan", merchant, plan_id]` | Usage-based pricing template (up to 5 tiers) |
| `VelaMandate` | `[b"mandate", subscriber, plan]` | Active subscription — tracks pull count, next due, billing type |
| `PullApproval` | `[b"approval", mandate]` | Per-period approval from Arcium; `valid_until = mandate.next_payment_due` |
| `UsageReport` | `[b"usage_report", mandate, period_start]` | Encrypted usage data submitted by merchant |
| `BillingEvent` | `[b"billing", mandate, pulls_executed]` | Encrypted audit record, no close authority |

## Arcium Circuits

Circuits are defined in `encrypted-ixs/src/lib.rs` and compiled to `build/`.

| Circuit | Encrypted Inputs | Output |
|---|---|---|
| `validate_mandate` | amount, balance, timestamp, next_due, expiry, pull count | `bool` (approved) |
| `usage_charge` | usage units, rate per unit, max charge | `u64` (capped charge) |
| `tiered_pricing` | usage units, 5 tier boundaries, 5 tier rates, tier count, cap | `u64` (capped charge) |
| `record_billing_event` | amount, timestamp, pulls, period start/end, payment method | MXE-encrypted `[u64; 8]` |

All circuits are written to satisfy MPC determinism constraints — no early returns, fixed-size loops, conditional assignment instead of branching.

**Build artifacts in `build/` (gitignored — regenerated via `arcium build`):**
- `*.arcis` — compiled circuit binary
- `*.arcis.ir` — intermediate representation
- `*.idarc` — Arcium circuit metadata
- `*.ts` — generated TypeScript type definitions
- `*.hash` / `*.weight` — content hash and resource estimate

Run `arcium build` before `anchor build` on a fresh clone — the program embeds `.arcis` bytecode at compile time via `include_bytes!`.

## Development

### Prerequisites

| Tool | Version |
|---|---|
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
# Everyday local build for anyone cloning this repo:
# validates repo-local program ids, then runs Arcium + Anchor
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

# Safe daily build for the real devnet program ids:
# restores the saved keypairs into target/deploy, then runs Arcium + Anchor
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

### Test

```sh
# Rust integration tests (LiteSVM, in-process)
cargo test -p vela-protocol -- --nocapture

# TypeScript integration tests (Bun + LiteSVM)
bun test ts-tests/

# Full CI pipeline (rebuilds Arcium circuits first)
bun run ci:protocol
```

### Deploy

```sh
# Safe devnet upgrade flow for the real deployed program
bun run deploy:devnet

# Initialize protocol config after deploy
# (see ts-tests/setup.ts for account initialization order)
```

Avoid raw `anchor deploy` for devnet. Use the guarded script path so the deployed program ids and upgrade flow stay aligned with `config/program-ids.json`.

## Program Architecture Notes

- **Credential NFTs** — Non-transferable Token-2022 mints issued per mandate, proving active subscription.
- **Billing type routing** — `VelaMandate.billing_type` is `Flat` or `Usage`. Usage mandates require a settled `UsageReport` before `request_validation` proceeds to `execute_pull`.
- **Keeper modes** — `Centralized` uses an off-chain keeper polling subscriptions. `TukTuk` routes through the on-chain task queue (not yet live).
- **Minimum frequency** — Plans must have `frequency ≥ 3600` seconds (1 hour).
- **Pending billing guard** — `execute_pull` is blocked if the previous pull has no finalized `BillingEvent`, enforcing sequential billing records.

## Repo Layout

```
programs/
  vela-protocol/       # Main billing program
  vela-transfer-hook/  # Token-2022 transfer hook validator
encrypted-ixs/          # Arcium circuit definitions (arcis DSL)
build/                  # Compiled circuit artifacts (gitignored)
tests/                  # Rust integration tests (13 tests, LiteSVM)
ts-tests/               # TypeScript integration tests (5 tests, Bun)
migrations/             # Anchor deploy migrations
```
