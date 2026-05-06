#!/usr/bin/env bash

read_devnet_program_ids() {
  python3 - <<'PY'
import json
from pathlib import Path

ids = json.loads(Path("config/program-ids.json").read_text())["devnet"]
print(ids["velaProtocol"])
print(ids["velaTransferHook"])
PY
}
