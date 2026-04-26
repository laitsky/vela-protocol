#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

CLUSTER="${CLUSTER:-devnet}"
RPC_URL="${RPC_URL:-https://api.devnet.solana.com}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

read_ids() {
python3 - <<'PY'
import json
from pathlib import Path
ids = json.loads(Path("config/program-ids.json").read_text())["devnet"]
print(ids["velaProtocol"])
print(ids["velaTransferHook"])
PY
}

compare_program() {
  local name="$1"
  local program_id="$2"
  local local_so="$3"
  local dumped_so="$TMP_DIR/${name}.so"

  echo "==> Dumping ${name} from ${CLUSTER}"
  solana program dump "$program_id" "$dumped_so" --url "$RPC_URL" >/dev/null

  local local_hash dumped_hash
  local_hash="$(shasum -a 256 "$local_so" | awk '{print $1}')"
  dumped_hash="$(shasum -a 256 "$dumped_so" | awk '{print $1}')"

  if [ "$local_hash" != "$dumped_hash" ]; then
    echo "ABORT: deployed ${name} bytes do not match local artifact." >&2
    echo "  local:    $local_hash  $local_so" >&2
    echo "  deployed: $dumped_hash  $program_id" >&2
    exit 1
  fi

  echo "    PASS ${name}: $local_hash"
}

compare_idl() {
  local name="$1"
  local program_id="$2"
  local local_idl="$3"
  local fetched_idl="$TMP_DIR/${name}.idl.json"

  echo "==> Fetching ${name} IDL from ${CLUSTER}"
  anchor idl fetch --provider.cluster "$CLUSTER" "$program_id" > "$fetched_idl"

  python3 - "$local_idl" "$fetched_idl" "$name" <<'PY'
import json
import sys
from pathlib import Path

local_path, fetched_path, name = sys.argv[1:4]
local = json.loads(Path(local_path).read_text())
fetched = json.loads(Path(fetched_path).read_text())

if local != fetched:
    print(f"ABORT: on-chain {name} IDL does not match local IDL.", file=sys.stderr)
    sys.exit(1)

print(f"    PASS {name} IDL")
PY
}

IDS="$(read_ids)"
PROTOCOL_PROGRAM_ID="$(printf '%s\n' "$IDS" | sed -n '1p')"
TRANSFER_HOOK_PROGRAM_ID="$(printf '%s\n' "$IDS" | sed -n '2p')"

compare_program "vela_protocol" "$PROTOCOL_PROGRAM_ID" "target/deploy/vela_protocol.so"
compare_program "vela_transfer_hook" "$TRANSFER_HOOK_PROGRAM_ID" "target/deploy/vela_transfer_hook.so"

compare_idl "vela_protocol" "$PROTOCOL_PROGRAM_ID" "target/idl/vela_protocol.json"
compare_idl "vela_transfer_hook" "$TRANSFER_HOOK_PROGRAM_ID" "target/idl/vela_transfer_hook.json"

echo "==> Deployed bytecode and IDLs match local artifacts"
