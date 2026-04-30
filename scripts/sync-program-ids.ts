import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  bytesForBase58,
  loadProgramIds,
  protocolRoot,
  replaceExact,
  replacePattern,
} from "./program-ids";

function updateFile(path: string, transform: (content: string) => string) {
  const previous = readFileSync(path, "utf8");
  const next = transform(previous);
  if (next !== previous) {
    writeFileSync(path, next);
  }
}

const manifest = loadProgramIds();
const devnet = manifest.devnet;
const localnet = manifest.localnet;
const hookBytes = bytesForBase58(devnet.velaTransferHook).join(", ");

updateFile(resolve(protocolRoot, "Anchor.toml"), (content) => {
  let next = replacePattern(
    content,
    /(\[programs\.devnet\]\s+vela_protocol = ")([^"]+)(")/,
    `$1${devnet.velaProtocol}$3`,
    "Anchor.toml",
  );
  next = replacePattern(
    next,
    /(\[programs\.devnet\][\s\S]*?vela_transfer_hook = ")([^"]+)(")/,
    `$1${devnet.velaTransferHook}$3`,
    "Anchor.toml",
  );
  next = replacePattern(
    next,
    /(\[programs\.localnet\]\s+vela_protocol = ")([^"]+)(")/,
    `$1${localnet.velaProtocol}$3`,
    "Anchor.toml",
  );
  return replacePattern(
    next,
    /(\[programs\.localnet\][\s\S]*?vela_transfer_hook = ")([^"]+)(")/,
    `$1${localnet.velaTransferHook}$3`,
    "Anchor.toml",
  );
});

updateFile(resolve(protocolRoot, "programs/vela-protocol/src/lib.rs"), (content) =>
  replacePattern(
    content,
    /declare_id!\("([^"]+)"\);/,
    `declare_id!("${devnet.velaProtocol}");`,
    "programs/vela-protocol/src/lib.rs",
  ),
);

updateFile(resolve(protocolRoot, "programs/vela-transfer-hook/src/lib.rs"), (content) =>
  replacePattern(
    content,
    /declare_id!\("([^"]+)"\);/,
    `declare_id!("${devnet.velaTransferHook}");`,
    "programs/vela-transfer-hook/src/lib.rs",
  ),
);

updateFile(resolve(protocolRoot, "programs/vela-protocol/src/constants.rs"), (content) =>
  replacePattern(
    content,
    /pub const TRANSFER_HOOK_PROGRAM_ID_BYTES: \[u8; 32\] = \[[\s\S]*?\];/,
    `pub const TRANSFER_HOOK_PROGRAM_ID_BYTES: [u8; 32] = [${hookBytes}];`,
    "programs/vela-protocol/src/constants.rs",
  ),
);

updateFile(resolve(protocolRoot, "README.md"), (content) => {
  let next = replacePattern(
    content,
    /(\| `vela-protocol` \| `)[^`]+(` \| Core billing logic[:—] plans, mandates, Arcium callbacks \|)/,
    `$1${devnet.velaProtocol}$2`,
    "README.md",
  );
  next = replacePattern(
    next,
    /(\| `vela-transfer-hook` \| `)[^`]+(` \| Token-2022 transfer hook[:—] enforces PullApproval on every transfer \|)/,
    `$1${devnet.velaTransferHook}$2`,
    "README.md",
  );
  return next;
});

updateFile(resolve(protocolRoot, "ts-tests/setup.ts"), (content) =>
  content.includes("const TRANSFER_HOOK_PROGRAM_ID = new PublicKey(transferHookIdl.address);")
    ? content
    : replaceExact(
        content,
        'const TRANSFER_HOOK_PROGRAM_ID = new PublicKey("93q91TJ6M9yGoehAeeCttgEc1SThFGXaw4rZS2ysr3uX");',
        "const TRANSFER_HOOK_PROGRAM_ID = new PublicKey(transferHookIdl.address);",
        "ts-tests/setup.ts",
      ),
);

updateFile(resolve(protocolRoot, "ts-tests/setup.ts"), (content) =>
  content.includes('import transferHookIdl from "../target/idl/vela_transfer_hook.json";')
    ? content
    : replaceExact(
        content,
        'import idl from "../target/idl/vela_protocol.json";',
        'import idl from "../target/idl/vela_protocol.json";\nimport transferHookIdl from "../target/idl/vela_transfer_hook.json";',
        "ts-tests/setup.ts",
      ),
);

console.log("Program ID manifest synced into vela-protocol repo files.");
