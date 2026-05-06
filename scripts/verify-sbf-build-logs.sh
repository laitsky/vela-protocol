#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
. scripts/lib/sbf-logs.sh

SBF_LOG_DIR="${SBF_BUILD_LOG_DIR:-target/sbf-build-logs}"
CHECKED_LOG_DIR="${CHECKED_LOG_DIR:-target/checked-build-logs}"
LOG_DIRS=("$SBF_LOG_DIR" "$CHECKED_LOG_DIR")
LOG_FILES=()
SBF_LOG_FILES=()
CHECKED_LOG_FILES=()
for dir in "${LOG_DIRS[@]}"; do
  if [ ! -d "$dir" ]; then
    continue
  fi

  shopt -s nullglob
  files=("${dir}"/*.log)
  shopt -u nullglob
  LOG_FILES+=("${files[@]}")
  if [ "$dir" = "$SBF_LOG_DIR" ]; then
    SBF_LOG_FILES+=("${files[@]}")
  fi
  if [ "$dir" = "$CHECKED_LOG_DIR" ]; then
    CHECKED_LOG_FILES+=("${files[@]}")
  fi
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

if [ "${REQUIRE_SBF_BUILD_LOGS:-0}" = "1" ] && [ "${#SBF_LOG_FILES[@]}" -eq 0 ]; then
  echo "ABORT: no final deployable SBF build log files found." >&2
  echo "Checked directory: ${SBF_LOG_DIR}" >&2
  echo "Run bun run build:devnet-safe before release verification." >&2
  exit 1
fi

if [ "${#SBF_LOG_FILES[@]}" -gt 0 ] \
  && grep -H -E "$STACK_WARNING_RE" "${SBF_LOG_FILES[@]}" >/tmp/vela-sbf-stack-warnings.txt; then
  if [ "${ALLOW_SBF_STACK_WARNINGS:-0}" = "1" ]; then
    cat >&2 <<EOF
WARNING: SBF stack-frame warning found in final deployable build logs.

ALLOW_SBF_STACK_WARNINGS=1 is set, so this diagnostic verification will continue.
Do not use this override for public releases or mainnet.

Matches:
EOF
    cat /tmp/vela-sbf-stack-warnings.txt >&2
    exit 0
  fi

  cat >&2 <<EOF
ABORT: SBF stack-frame warning found in final deployable build logs.

This is a release blocker for normal upgrades. Do not deploy these artifacts
until the warning is removed or an audited mitigation is documented.

Matches:
EOF
  cat /tmp/vela-sbf-stack-warnings.txt >&2
  exit 1
fi

if [ "${#CHECKED_LOG_FILES[@]}" -gt 0 ] \
  && grep -H -E "$STACK_WARNING_RE" "${CHECKED_LOG_FILES[@]}" >/tmp/vela-checked-stack-warnings.txt; then
  if [ "${ALLOW_SBF_STACK_WARNINGS:-0}" = "1" ]; then
    cat >&2 <<EOF
WARNING: SBF stack-frame warning found in checked command logs.

ALLOW_SBF_STACK_WARNINGS=1 is set, so this diagnostic verification will continue.
Do not use this override for public releases or mainnet.

Matches:
EOF
    cat /tmp/vela-checked-stack-warnings.txt >&2
    exit 0
  fi

  if grep -H -E "$STACK_WARNING_RE" "${CHECKED_LOG_FILES[@]}" \
    | grep -Ev "^${CHECKED_LOG_DIR}/arcium-build[^:]*\\.log:.*${KNOWN_ARCIUM_IDL_STACK_RE}" >/tmp/vela-unknown-checked-stack-warnings.txt; then
    cat >&2 <<EOF
ABORT: unexpected SBF stack-frame warning found in checked command logs.

Only the documented Arcium generated IDL warning is allowed in arcium-build
checked logs. Final deployable SBF logs are still release-blocking.

Matches:
EOF
    cat /tmp/vela-unknown-checked-stack-warnings.txt >&2
    exit 1
  fi

  cat >&2 <<EOF
WARNING: known Arcium generated IDL stack-frame warning found in checked command logs.

This exception is limited to arcium_client::idl::arcium::utils::Account::try_from
inside arcium-build logs. Final deployable SBF logs contain no blocked stack-frame
warnings.

Matches:
EOF
  cat /tmp/vela-checked-stack-warnings.txt >&2
fi

echo "==> Final deployable SBF build logs contain no blocked stack-frame warnings"
