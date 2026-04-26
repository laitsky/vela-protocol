#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

RPC_URL="${RPC_URL:-https://api.devnet.solana.com}"

run() {
  echo "==> $*"
  "$@"
}

require_file() {
  if [ ! -f "$1" ]; then
    echo "ABORT: missing required file: $1" >&2
    exit 1
  fi
}

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

echo "Vela rollout verification"
echo "  RPC:                 $RPC_URL"
echo "  vela_protocol:       $PROTOCOL_PROGRAM_ID"
echo "  vela_transfer_hook:  $TRANSFER_HOOK_PROGRAM_ID"

run bun run program-ids:check
run bash scripts/prepare-program-keys.sh

require_file target/deploy/vela_protocol.so
require_file target/deploy/vela_transfer_hook.so
require_file target/idl/vela_protocol.json
require_file target/idl/vela_transfer_hook.json

if [ -d ../vela-sdk ]; then
  echo "==> Verifying SDK IDL copies"
  require_file ../vela-sdk/idl/vela_protocol.json
  require_file ../vela-sdk/idl/vela_transfer_hook.json
  protocol_idl_hash="$(shasum -a 256 target/idl/vela_protocol.json | awk '{print $1}')"
  sdk_protocol_idl_hash="$(shasum -a 256 ../vela-sdk/idl/vela_protocol.json | awk '{print $1}')"
  hook_idl_hash="$(shasum -a 256 target/idl/vela_transfer_hook.json | awk '{print $1}')"
  sdk_hook_idl_hash="$(shasum -a 256 ../vela-sdk/idl/vela_transfer_hook.json | awk '{print $1}')"

  if [ "$protocol_idl_hash" != "$sdk_protocol_idl_hash" ]; then
    echo "ABORT: vela_protocol IDL differs between protocol and SDK." >&2
    echo "  protocol: $protocol_idl_hash" >&2
    echo "  sdk:      $sdk_protocol_idl_hash" >&2
    exit 1
  fi

  if [ "$hook_idl_hash" != "$sdk_hook_idl_hash" ]; then
    echo "ABORT: vela_transfer_hook IDL differs between protocol and SDK." >&2
    echo "  protocol: $hook_idl_hash" >&2
    echo "  sdk:      $sdk_hook_idl_hash" >&2
    exit 1
  fi
fi

check_upgrade_authority() {
  local name="$1"
  local program_id="$2"

  if ! solana program show "$program_id" --url "$RPC_URL" >/tmp/vela-program-show.txt 2>/dev/null; then
    echo "==> ${name} is not deployed yet; skipping upgrade-authority check"
    return
  fi

  local wallet authority
  wallet="$(solana-keygen pubkey)"
  authority="$(awk -F': ' '/^Authority:/ {print $2}' /tmp/vela-program-show.txt)"
  if [ "$authority" != "$wallet" ]; then
    echo "ABORT: current wallet is not the upgrade authority for ${name}." >&2
    echo "  current wallet: $wallet" >&2
    echo "  authority:      $authority" >&2
    exit 1
  fi
}

check_upgrade_authority "vela_protocol" "$PROTOCOL_PROGRAM_ID"
check_upgrade_authority "vela_transfer_hook" "$TRANSFER_HOOK_PROGRAM_ID"

if [ "${SKIP_CONSUMER_CHECKS:-0}" != "1" ]; then
  if [ -d ../vela-sdk ]; then
    run bash -c 'cd ../vela-sdk && bun run check'
    run bash -c 'cd ../vela-sdk && bun test tests/unit/idl.test.ts src/events/__tests__'
  fi

  if [ -d ../vela-webhook ]; then
    run bash -c 'cd ../vela-webhook && bun run build && bun test'
  fi

  if [ -d ../vela-dashboard ]; then
    run bash -c 'cd ../vela-dashboard && bun test worker/src/routes/webhook.test.ts worker/src/queue/webhook-fanout.test.ts worker/src/queue/helius-webhook-upgrade-events.test.ts worker/tests/stream-events.test.ts'
  fi

  if [ -d ../vela-checkout ]; then
    run bash -c 'cd ../vela-checkout && bun run check'
  fi

  if [ -d ../vela-widget ]; then
    run bash -c 'cd ../vela-widget && bun run typecheck'
  fi
else
  echo "==> SKIP_CONSUMER_CHECKS=1; skipping SDK/dashboard/webhook/checkout/widget checks"
fi

echo "==> Rollout verification passed"
