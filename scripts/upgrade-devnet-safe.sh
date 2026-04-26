#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "Vela devnet upgrade"
echo "──────────────────"

echo "==> Building devnet artifacts"
bash scripts/build-devnet-safe.sh

echo "==> Running pre-upgrade rollout checks"
bash scripts/verify-rollout.sh

echo "==> Deploying/upgrading devnet programs"
SKIP_BUILD=1 bash scripts/deploy-devnet.sh

echo "==> Verifying deployed artifacts"
bash scripts/assert-deployed-hash.sh

cat <<'EOF'

Vela devnet upgrade report
──────────────────────────
Build:             PASS
Program IDs:       PASS
IDL sync:          PASS
Consumer checks:   PASS
Deploy/upgrade:    PASS
Byte hash:         PASS
On-chain IDL:      PASS

Safe to roll consumers: yes
EOF
