#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

KEY_DIR="${VELA_PROGRAM_KEY_DIR:-$HOME/.config/velapay/keys/devnet}"
OVERWRITE="${VELA_PROGRAM_KEY_OVERWRITE:-0}"

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

SOURCE_PROTOCOL="target/deploy/vela_protocol-keypair.json"
SOURCE_TRANSFER_HOOK="target/deploy/vela_transfer_hook-keypair.json"

if [ ! -f "$SOURCE_PROTOCOL" ] || [ ! -f "$SOURCE_TRANSFER_HOOK" ]; then
  cat >&2 <<EOF
ABORT: target/deploy program keypairs are missing.

Expected:
  $SOURCE_PROTOCOL
  $SOURCE_TRANSFER_HOOK

Restore the correct keypairs first, then run:
  bun run keys:backup:devnet
EOF
  exit 1
fi

ACTUAL_PROTOCOL_ID="$(solana address -k "$SOURCE_PROTOCOL")"
ACTUAL_TRANSFER_HOOK_ID="$(solana address -k "$SOURCE_TRANSFER_HOOK")"

if [ "$ACTUAL_PROTOCOL_ID" != "$EXPECTED_PROTOCOL_ID" ]; then
  echo "ABORT: vela_protocol keypair does not match config/program-ids.json." >&2
  echo "  expected: $EXPECTED_PROTOCOL_ID" >&2
  echo "  actual:   $ACTUAL_PROTOCOL_ID" >&2
  exit 1
fi

if [ "$ACTUAL_TRANSFER_HOOK_ID" != "$EXPECTED_TRANSFER_HOOK_ID" ]; then
  echo "ABORT: vela_transfer_hook keypair does not match config/program-ids.json." >&2
  echo "  expected: $EXPECTED_TRANSFER_HOOK_ID" >&2
  echo "  actual:   $ACTUAL_TRANSFER_HOOK_ID" >&2
  exit 1
fi

mkdir -p "$KEY_DIR"

backup_one() {
  local source_path="$1"
  local target_path="$2"

  if [ -e "$target_path" ] && [ "$OVERWRITE" != "1" ]; then
    cat >&2 <<EOF
ABORT: $target_path already exists.

Refusing to overwrite a saved program keypair by default.
If you intentionally want to replace it, rerun with:
  VELA_PROGRAM_KEY_OVERWRITE=1 bun run keys:backup:devnet
EOF
    exit 1
  fi

  install -m 600 "$source_path" "$target_path"
}

backup_one "$SOURCE_PROTOCOL" "$KEY_DIR/vela_protocol-keypair.json"
backup_one "$SOURCE_TRANSFER_HOOK" "$KEY_DIR/vela_transfer_hook-keypair.json"

cat <<EOF
Saved persistent devnet program keypairs to:
  $KEY_DIR/vela_protocol-keypair.json
  $KEY_DIR/vela_transfer_hook-keypair.json

Future safe builds can now restore from this directory with:
  bun run build:devnet-safe
EOF
