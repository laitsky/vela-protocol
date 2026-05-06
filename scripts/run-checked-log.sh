#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "usage: $0 <name> <command> [args...]" >&2
  exit 2
fi

NAME="$1"
shift

cd "$(dirname "$0")/.."
. scripts/lib/sbf-logs.sh

LOG_DIR="${CHECKED_LOG_DIR:-target/checked-build-logs}"
LOG_PATH="${LOG_DIR}/${NAME}.log"
mkdir -p "$LOG_DIR"

echo "==> Running ${NAME}"

if ! "$@" 2>&1 | tee "$LOG_PATH"; then
  echo "ABORT: command failed for ${NAME}. See ${LOG_PATH}" >&2
  exit 1
fi

if grep -E "$STACK_WARNING_RE" "$LOG_PATH" >/dev/null; then
  if [[ "$NAME" == arcium-build* ]] \
    && ! grep -E "$STACK_WARNING_RE" "$LOG_PATH" | grep -Ev "$KNOWN_ARCIUM_IDL_STACK_RE" >/dev/null; then
    cat >&2 <<EOF
WARNING: known Arcium generated IDL stack-frame warning detected while running ${NAME}.

The command completed successfully and this warning is allowed only for
arcium_client::idl::arcium::utils::Account::try_from in Arcium checked-build
logs. Final deployable SBF artifacts are still verified separately.

Build log: ${LOG_PATH}
EOF
    exit 0
  fi

  if [ "${ALLOW_SBF_STACK_WARNINGS:-0}" = "1" ]; then
    cat >&2 <<EOF
WARNING: SBF stack-frame warning detected while running ${NAME}.

ALLOW_SBF_STACK_WARNINGS=1 is set, so this diagnostic/devnet command will continue.
Do not use this override for public releases or mainnet.

Build log: ${LOG_PATH}
EOF
    exit 0
  fi

  cat >&2 <<EOF
ABORT: SBF stack-frame warning detected while running ${NAME}.

This is treated as a release blocker because the produced binary may be rejected
or may fail at runtime under the Solana loader. Do not ship this artifact until
the warning is removed or an audited upstream/vendor mitigation is documented.

Build log: ${LOG_PATH}

For local diagnostics only, rerun with:
  ALLOW_SBF_STACK_WARNINGS=1 <command>
EOF
  exit 1
fi
