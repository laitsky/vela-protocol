#!/usr/bin/env bash

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
