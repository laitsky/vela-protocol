#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Checking repo-local program IDs"
bun run program-ids:check

echo "==> Compiling Arcium circuits"
arcium build

echo "==> Building Anchor programs"
anchor build

echo "==> Local build complete"
