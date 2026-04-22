#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

KEY_DIR="${VELA_PROGRAM_KEY_DIR:-$HOME/.config/velapay/keys/devnet}"

if ! command -v solana >/dev/null 2>&1; then
  echo "ABORT: solana CLI is required to verify program keypairs." >&2
  exit 1
fi

read_expected_ids() {
  python3 - <<'PY'
import json
from pathlib import Path

data = json.loads(Path("config/program-ids.json").read_text())
print(data["devnet"]["velaProtocol"])
print(data["devnet"]["velaTransferHook"])
PY
}

EXPECTED_IDS="$(read_expected_ids)"
EXPECTED_PROTOCOL_ID="$(printf '%s\n' "$EXPECTED_IDS" | sed -n '1p')"
EXPECTED_TRANSFER_HOOK_ID="$(printf '%s\n' "$EXPECTED_IDS" | sed -n '2p')"

SOURCE_PROTOCOL="$KEY_DIR/vela_protocol-keypair.json"
SOURCE_TRANSFER_HOOK="$KEY_DIR/vela_transfer_hook-keypair.json"

if [ ! -f "$SOURCE_PROTOCOL" ] || [ ! -f "$SOURCE_TRANSFER_HOOK" ]; then
  cat >&2 <<EOF
ABORT: persistent devnet program keypairs are missing.

Expected:
  $SOURCE_PROTOCOL
  $SOURCE_TRANSFER_HOOK

One-time setup:
  bun run keys:backup:devnet

Or point to a different directory:
  VELA_PROGRAM_KEY_DIR=/secure/path/to/devnet-keys bun run build:devnet-safe
EOF
  exit 1
fi

ACTUAL_PROTOCOL_ID="$(solana address -k "$SOURCE_PROTOCOL")"
ACTUAL_TRANSFER_HOOK_ID="$(solana address -k "$SOURCE_TRANSFER_HOOK")"

if [ "$ACTUAL_PROTOCOL_ID" != "$EXPECTED_PROTOCOL_ID" ]; then
  echo "ABORT: saved vela_protocol keypair does not match config/program-ids.json." >&2
  echo "  expected: $EXPECTED_PROTOCOL_ID" >&2
  echo "  actual:   $ACTUAL_PROTOCOL_ID" >&2
  exit 1
fi

if [ "$ACTUAL_TRANSFER_HOOK_ID" != "$EXPECTED_TRANSFER_HOOK_ID" ]; then
  echo "ABORT: saved vela_transfer_hook keypair does not match config/program-ids.json." >&2
  echo "  expected: $EXPECTED_TRANSFER_HOOK_ID" >&2
  echo "  actual:   $ACTUAL_TRANSFER_HOOK_ID" >&2
  exit 1
fi

mkdir -p target/deploy
install -m 600 "$SOURCE_PROTOCOL" "target/deploy/vela_protocol-keypair.json"
install -m 600 "$SOURCE_TRANSFER_HOOK" "target/deploy/vela_transfer_hook-keypair.json"

cat <<EOF
Restored devnet program keypairs into target/deploy:
  target/deploy/vela_protocol-keypair.json
  target/deploy/vela_transfer_hook-keypair.json
EOF
