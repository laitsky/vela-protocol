#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

SBF_LOG_DIR="${SBF_BUILD_LOG_DIR:-target/sbf-build-logs}"
CHECKED_LOG_DIR="${CHECKED_LOG_DIR:-target/checked-build-logs}"
LOG_DIRS=("$SBF_LOG_DIR" "$CHECKED_LOG_DIR")
LOG_FILES=()

for dir in "${LOG_DIRS[@]}"; do
  if [ ! -d "$dir" ]; then
    continue
  fi

  shopt -s nullglob
  files=("${dir}"/*.log)
  shopt -u nullglob
  LOG_FILES+=("${files[@]}")
done

if [ "${#LOG_FILES[@]}" -eq 0 ]; then
  if [ "${REQUIRE_SBF_BUILD_LOGS:-0}" = "1" ]; then
    echo "ABORT: no checked SBF build log files found." >&2
    echo "Checked directories:" >&2
    printf '  - %s\n' "${LOG_DIRS[@]}" >&2
    echo "Run bun run build:devnet-safe before release verification." >&2
    exit 1
  fi

  echo "==> No checked SBF build log files found; skipping stack-warning scan"
  exit 0
fi

if grep -H -E "Stack offset of [0-9]+ exceeded max offset of 4096" "${LOG_FILES[@]}" >/tmp/vela-sbf-stack-warnings.txt; then
  if [ "${ALLOW_SBF_STACK_WARNINGS:-0}" = "1" ]; then
    cat >&2 <<EOF
WARNING: SBF stack-frame warning found in build logs.

ALLOW_SBF_STACK_WARNINGS=1 is set, so this diagnostic verification will continue.
Do not use this override for public releases or mainnet.

Matches:
EOF
    cat /tmp/vela-sbf-stack-warnings.txt >&2
    exit 0
  fi

  cat >&2 <<EOF
ABORT: SBF stack-frame warning found in build logs.

This is treated as a release blocker for normal upgrades. Do not deploy these
artifacts until the warning is removed or an audited mitigation is documented.

Matches:
EOF
  cat /tmp/vela-sbf-stack-warnings.txt >&2
  exit 1
fi

echo "==> Checked SBF build logs contain no blocked stack-frame warnings"
