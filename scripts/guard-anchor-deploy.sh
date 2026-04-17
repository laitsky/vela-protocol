#!/usr/bin/env bash
# Refuse any `anchor deploy` that would target the wrong program id.
# Run before CI or wire into a pre-commit / shell alias.
set -euo pipefail

cd "$(dirname "$0")/.."

declared=$(grep -oE 'declare_id!\("[A-Za-z0-9]+"\)' programs/vela-protocol/src/lib.rs | grep -oE '"[A-Za-z0-9]+"' | tr -d '"')
keypair_pub=""
if [ -f target/deploy/vela_protocol-keypair.json ]; then
  keypair_pub=$(solana address -k target/deploy/vela_protocol-keypair.json)
fi

if [ -n "$keypair_pub" ] && [ "$keypair_pub" != "$declared" ]; then
  cat >&2 <<EOF
REFUSING to allow 'anchor deploy' for vela_protocol.

  declare_id!()              = $declared
  target/deploy/*-keypair    = $keypair_pub

The original program keypair is LOST. Running 'anchor deploy' will deploy a
NEW program at the keypair address, not upgrade the real one.

Use scripts/deploy-devnet.sh (buffer upload + upgrade) instead.
EOF
  exit 1
fi
echo "OK: deploy keypair matches declare_id!() ($declared)"
