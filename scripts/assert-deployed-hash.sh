#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
. scripts/lib/program-ids.sh

CLUSTER="${CLUSTER:-devnet}"
RPC_URL="${RPC_URL:-https://api.devnet.solana.com}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

compare_program() {
  local name="$1"
  local program_id="$2"
  local local_so="$3"
  local dumped_so="$TMP_DIR/${name}.so"

  echo "==> Dumping ${name} from ${CLUSTER}"
  solana program dump "$program_id" "$dumped_so" --url "$RPC_URL" >/dev/null

  python3 - "$local_so" "$dumped_so" "$name" "$program_id" <<'PY'
import hashlib
import sys
from pathlib import Path

local_path, dumped_path, name, program_id = sys.argv[1:5]
local = Path(local_path).read_bytes()
dumped = Path(dumped_path).read_bytes()

local_hash = hashlib.sha256(local).hexdigest()
dumped_prefix = dumped[: len(local)]
prefix_hash = hashlib.sha256(dumped_prefix).hexdigest()

if dumped_prefix != local:
    print(f"ABORT: deployed {name} bytes do not match local artifact.", file=sys.stderr)
    print(f"  local:    {local_hash}  {local_path}", file=sys.stderr)
    print(f"  deployed: {prefix_hash}  {program_id} prefix[{len(local)}]", file=sys.stderr)
    sys.exit(1)

padding = dumped[len(local):]
if any(padding):
    print(f"ABORT: deployed {name} has non-zero trailing ProgramData padding.", file=sys.stderr)
    print(f"  local size:    {len(local)} bytes", file=sys.stderr)
    print(f"  deployed size: {len(dumped)} bytes", file=sys.stderr)
    sys.exit(1)

extra = len(dumped) - len(local)
suffix = f", {extra} zero padding bytes" if extra else ""
print(f"    PASS {name}: {local_hash}{suffix}")
PY
}

compare_idl() {
  local name="$1"
  local program_id="$2"
  local local_idl="$3"

  echo "==> Fetching ${name} PMP IDL from ${CLUSTER}"

  node - "$program_id" "$local_idl" "$name" "$RPC_URL" <<'NODE'
const { readFileSync } = require("fs");
const { inflateSync } = require("zlib");
const { Connection, PublicKey } = require("@solana/web3.js");

const [programId, localPath, name, rpcUrl] = process.argv.slice(2);
const PROGRAM_METADATA_PROGRAM_ID = new PublicKey("ProgM6JCCvbYkfKqJYHePx4xxSUSqJp7rh8Lyv7nk7S");
const IDL_SEED = Buffer.concat([Buffer.from("idl"), Buffer.alloc(13)]);
const HEADER_LENGTH = 96;

(async () => {
  const local = JSON.parse(readFileSync(localPath, "utf8"));
  const [metadata] = PublicKey.findProgramAddressSync(
    [new PublicKey(programId).toBuffer(), IDL_SEED],
    PROGRAM_METADATA_PROGRAM_ID,
  );
  const account = await new Connection(rpcUrl, "confirmed").getAccountInfo(metadata);

  if (!account) {
    throw new Error(`ABORT: on-chain ${name} PMP IDL account does not exist (${metadata.toBase58()}).`);
  }
  if (!account.owner.equals(PROGRAM_METADATA_PROGRAM_ID)) {
    throw new Error(`ABORT: on-chain ${name} PMP IDL account has unexpected owner.`);
  }

  const payload = account.data.subarray(HEADER_LENGTH);
  const trimmed = payload.subarray(0, payload.findLastIndex((byte) => byte !== 0) + 1);
  const fetched = JSON.parse(inflateSync(trimmed).toString("utf8"));

  if (JSON.stringify(local) !== JSON.stringify(fetched)) {
    throw new Error(`ABORT: on-chain ${name} PMP IDL does not match local IDL.`);
  }

  console.log(`    PASS ${name} PMP IDL`);
})().catch((error) => {
  console.error(error.message || error);
  process.exit(1);
});
NODE
}

IDS="$(read_devnet_program_ids)"
PROTOCOL_PROGRAM_ID="$(printf '%s\n' "$IDS" | sed -n '1p')"
TRANSFER_HOOK_PROGRAM_ID="$(printf '%s\n' "$IDS" | sed -n '2p')"

compare_program "vela_protocol" "$PROTOCOL_PROGRAM_ID" "target/deploy/vela_protocol.so"
compare_program "vela_transfer_hook" "$TRANSFER_HOOK_PROGRAM_ID" "target/deploy/vela_transfer_hook.so"

compare_idl "vela_protocol" "$PROTOCOL_PROGRAM_ID" "target/idl/vela_protocol.json"
compare_idl "vela_transfer_hook" "$TRANSFER_HOOK_PROGRAM_ID" "target/idl/vela_transfer_hook.json"

echo "==> Deployed bytecode and IDLs match local artifacts"
