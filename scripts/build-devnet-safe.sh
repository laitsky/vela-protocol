#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Checking program ID manifest and generated consumers"
bun run program-ids:check

echo "==> Restoring persistent devnet program keypairs"
bash scripts/prepare-program-keys.sh

echo "==> Compiling Arcium circuits"
arcium build

echo "==> Building Anchor programs"
anchor build

echo "==> Verifying restored keypairs still match the expected devnet ids"
bash scripts/prepare-program-keys.sh >/dev/null

echo "==> Safe devnet build complete"
