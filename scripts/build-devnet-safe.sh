#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

PROTOCOL_SBF_ARCH="${PROTOCOL_SBF_ARCH:-v2}"
TRANSFER_HOOK_SBF_ARCH="${TRANSFER_HOOK_SBF_ARCH:-v2}"

echo "==> Checking program ID manifest and generated consumers"
bun run program-ids:check

echo "==> Restoring persistent devnet program keypairs"
bash scripts/prepare-program-keys.sh

echo "==> Compiling Arcium circuits"
arcium build

echo "==> Building vela_protocol with arch ${PROTOCOL_SBF_ARCH}"
cargo-build-sbf --manifest-path programs/vela-protocol/Cargo.toml --arch "${PROTOCOL_SBF_ARCH}"

echo "==> Building vela_transfer_hook with arch ${TRANSFER_HOOK_SBF_ARCH}"
cargo-build-sbf --manifest-path programs/vela-transfer-hook/Cargo.toml --arch "${TRANSFER_HOOK_SBF_ARCH}"

if [ ! -f target/idl/vela_protocol.json ] || [ ! -f target/idl/vela_transfer_hook.json ]; then
  echo "ABORT: expected IDL artifacts are missing from target/idl." >&2
  echo "Populate target/idl before running a safe devnet build." >&2
  exit 1
fi

echo "==> Verifying restored keypairs still match the expected devnet ids"
bash scripts/prepare-program-keys.sh >/dev/null

echo "==> Safe devnet build complete"
