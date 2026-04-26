#!/usr/bin/env bash
set -euo pipefail

# Safe devnet deploy or upgrade for the Vela programs.
# We intentionally preserve the original devnet program keypairs and always deploy
# against those addresses instead of letting Anchor create new random program ids.

read_ids() {
python3 - <<'PY'
import json
from pathlib import Path
ids = json.loads(Path("config/program-ids.json").read_text())["devnet"]
print(ids["velaProtocol"])
print(ids["velaTransferHook"])
PY
}

IDS="$(read_ids)"
PROTOCOL_PROGRAM_ID="$(printf '%s\n' "$IDS" | sed -n '1p')"
TRANSFER_HOOK_PROGRAM_ID="$(printf '%s\n' "$IDS" | sed -n '2p')"

CLUSTER="${CLUSTER:-devnet}"
RPC_URL="${RPC_URL:-https://api.devnet.solana.com}"
PROTOCOL_SBF_ARCH="${PROTOCOL_SBF_ARCH:-v2}"
TRANSFER_HOOK_SBF_ARCH="${TRANSFER_HOOK_SBF_ARCH:-v2}"

cd "$(dirname "$0")/.."

declared_protocol=$(grep -oE 'declare_id!\("[A-Za-z0-9]+"\)' programs/vela-protocol/src/lib.rs | grep -oE '"[A-Za-z0-9]+"' | tr -d '"')
declared_transfer_hook=$(grep -oE 'declare_id!\("[A-Za-z0-9]+"\)' programs/vela-transfer-hook/src/lib.rs | grep -oE '"[A-Za-z0-9]+"' | tr -d '"')

if [ "$declared_protocol" != "$PROTOCOL_PROGRAM_ID" ]; then
  echo "ABORT: vela_protocol declare_id!() is $declared_protocol but manifest targets $PROTOCOL_PROGRAM_ID." >&2
  exit 1
fi

if [ "$declared_transfer_hook" != "$TRANSFER_HOOK_PROGRAM_ID" ]; then
  echo "ABORT: vela_transfer_hook declare_id!() is $declared_transfer_hook but manifest targets $TRANSFER_HOOK_PROGRAM_ID." >&2
  exit 1
fi

echo "==> Restoring persistent devnet program keypairs"
bash scripts/prepare-program-keys.sh >/dev/null

if [ "${SKIP_BUILD:-0}" != "1" ]; then
  echo "==> Compiling Arcium circuits"
  arcium build

  echo "==> Building vela_protocol with arch ${PROTOCOL_SBF_ARCH}"
  cargo-build-sbf --manifest-path programs/vela-protocol/Cargo.toml --arch "${PROTOCOL_SBF_ARCH}"

  echo "==> Building vela_transfer_hook with arch ${TRANSFER_HOOK_SBF_ARCH}"
  cargo-build-sbf --manifest-path programs/vela-transfer-hook/Cargo.toml --arch "${TRANSFER_HOOK_SBF_ARCH}"
else
  echo "==> SKIP_BUILD=1; using existing target/deploy artifacts"
fi

if [ ! -f target/idl/vela_protocol.json ] || [ ! -f target/idl/vela_transfer_hook.json ]; then
  echo "ABORT: expected IDL artifacts are missing from target/idl." >&2
  echo "Populate target/idl before deploying." >&2
  exit 1
fi

deploy_or_upgrade() {
  local name="$1"
  local program_id="$2"
  local so_path="$3"
  local keypair_path="$4"

  if solana program show "$program_id" --url "$RPC_URL" >/dev/null 2>&1; then
    echo "==> Upgrading ${name} at ${program_id}"
    local buffer
    buffer=$(solana program write-buffer "$so_path" --url "$RPC_URL" | awk '/^Buffer:/ {print $2}')
    if [ -z "$buffer" ]; then
      echo "ABORT: failed to capture buffer address for ${name}" >&2
      exit 1
    fi
    echo "    Buffer: $buffer"

    if ! solana program upgrade "$buffer" "$program_id" --url "$RPC_URL"; then
      echo "Upgrade failed for ${name}. If 'ProgramData account not large enough', run:" >&2
      echo "  solana program extend $program_id <bytes> --url $RPC_URL" >&2
      echo "Then re-upgrade the buffer:" >&2
      echo "  solana program upgrade $buffer $program_id --url $RPC_URL" >&2
      exit 1
    fi
  else
    echo "==> Initial deploy for ${name} at ${program_id}"
    solana program deploy "$so_path" \
      --program-id "$keypair_path" \
      --url "$RPC_URL"
  fi
}

deploy_or_upgrade \
  "vela_transfer_hook" \
  "$TRANSFER_HOOK_PROGRAM_ID" \
  "target/deploy/vela_transfer_hook.so" \
  "target/deploy/vela_transfer_hook-keypair.json"

deploy_or_upgrade \
  "vela_protocol" \
  "$PROTOCOL_PROGRAM_ID" \
  "target/deploy/vela_protocol.so" \
  "target/deploy/vela_protocol-keypair.json"

echo "==> Refreshing on-chain vela_protocol IDL"
anchor idl upgrade --provider.cluster "$CLUSTER" --filepath "target/idl/vela_protocol.json" "$PROTOCOL_PROGRAM_ID" || {
  echo "IDL upgrade failed for vela_protocol (likely first deploy or size change). Re-initializing..."
  anchor idl close --provider.cluster "$CLUSTER" "$PROTOCOL_PROGRAM_ID" || true
  anchor idl init --provider.cluster "$CLUSTER" --filepath "target/idl/vela_protocol.json" "$PROTOCOL_PROGRAM_ID"
}

echo "==> Refreshing on-chain vela_transfer_hook IDL"
anchor idl upgrade --provider.cluster "$CLUSTER" --filepath "target/idl/vela_transfer_hook.json" "$TRANSFER_HOOK_PROGRAM_ID" || {
  echo "IDL upgrade failed for vela_transfer_hook (likely first deploy or size change). Re-initializing..."
  anchor idl close --provider.cluster "$CLUSTER" "$TRANSFER_HOOK_PROGRAM_ID" || true
  anchor idl init --provider.cluster "$CLUSTER" --filepath "target/idl/vela_transfer_hook.json" "$TRANSFER_HOOK_PROGRAM_ID"
}

echo "==> Done. Verify:"
solana program show "$TRANSFER_HOOK_PROGRAM_ID" --url "$RPC_URL"
solana program show "$PROTOCOL_PROGRAM_ID" --url "$RPC_URL"
