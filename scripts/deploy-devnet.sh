#!/usr/bin/env bash
set -euo pipefail

# Safe devnet upgrade for vela_protocol.
# The original program keypair is lost. The program is still upgradeable because
# our wallet is the upgrade authority. NEVER run `anchor deploy` for this program
# again — it would deploy to whatever keypair is in target/deploy/, which is no
# longer the real program id.

PROGRAM_ID=$(python3 - <<'PY'
import json
from pathlib import Path
print(json.loads(Path("config/program-ids.json").read_text())["devnet"]["velaProtocol"])
PY
)
SO_PATH="target/deploy/vela_protocol.so"
IDL_PATH="target/idl/vela_protocol.json"
CLUSTER="${CLUSTER:-devnet}"
RPC_URL="${RPC_URL:-https://api.devnet.solana.com}"

cd "$(dirname "$0")/.."

declared=$(grep -oE 'declare_id!\("[A-Za-z0-9]+"\)' programs/vela-protocol/src/lib.rs | grep -oE '"[A-Za-z0-9]+"' | tr -d '"')
if [ "$declared" != "$PROGRAM_ID" ]; then
  echo "ABORT: declare_id!() is $declared but this script targets $PROGRAM_ID." >&2
  echo "If you intentionally changed program ids, update this script." >&2
  exit 1
fi

echo "==> Compiling Arcium circuits"
arcium build

echo "==> Building Anchor programs"
anchor build

echo "==> Writing buffer to $CLUSTER"
buffer=$(solana program write-buffer "$SO_PATH" --url "$RPC_URL" | awk '/^Buffer:/ {print $2}')
if [ -z "$buffer" ]; then
  echo "ABORT: failed to capture buffer address" >&2
  exit 1
fi
echo "    Buffer: $buffer"

echo "==> Upgrading $PROGRAM_ID"
if ! solana program upgrade "$buffer" "$PROGRAM_ID" --url "$RPC_URL"; then
  echo "Upgrade failed. If 'ProgramData account not large enough', run:" >&2
  echo "  solana program extend $PROGRAM_ID <bytes> --url $RPC_URL" >&2
  echo "Then re-upgrade the buffer:" >&2
  echo "  solana program upgrade $buffer $PROGRAM_ID --url $RPC_URL" >&2
  exit 1
fi

echo "==> Refreshing on-chain IDL"
anchor idl upgrade --provider.cluster "$CLUSTER" --filepath "$IDL_PATH" "$PROGRAM_ID" || {
  echo "IDL upgrade failed (likely size grew). Closing and re-initing..."
  anchor idl close --provider.cluster "$CLUSTER" "$PROGRAM_ID"
  anchor idl init --provider.cluster "$CLUSTER" --filepath "$IDL_PATH" "$PROGRAM_ID"
}

echo "==> Done. Verify:"
solana program show "$PROGRAM_ID" --url "$RPC_URL"
