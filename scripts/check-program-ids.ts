import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { bytesForBase58, loadProgramIds, protocolRoot } from "./program-ids";

function assertIncludes(filePath: string, snippet: string, label: string) {
  const content = readFileSync(filePath, "utf8");
  if (!content.includes(snippet)) {
    throw new Error(`${label} mismatch in ${filePath}`);
  }
}

const manifest = loadProgramIds();
const devnet = manifest.devnet;
const localnet = manifest.localnet;
const hookBytes = bytesForBase58(devnet.velaTransferHook).join(", ");

assertIncludes(resolve(protocolRoot, "Anchor.toml"), `vela_protocol = "${devnet.velaProtocol}"`, "devnet protocol");
assertIncludes(resolve(protocolRoot, "Anchor.toml"), `vela_transfer_hook = "${devnet.velaTransferHook}"`, "devnet transfer hook");
assertIncludes(resolve(protocolRoot, "Anchor.toml"), `vela_protocol = "${localnet.velaProtocol}"`, "localnet protocol");
assertIncludes(resolve(protocolRoot, "Anchor.toml"), `vela_transfer_hook = "${localnet.velaTransferHook}"`, "localnet transfer hook");
assertIncludes(resolve(protocolRoot, "programs/vela-protocol/src/lib.rs"), `declare_id!("${devnet.velaProtocol}");`, "protocol declare_id");
assertIncludes(resolve(protocolRoot, "programs/vela-transfer-hook/src/lib.rs"), `declare_id!("${devnet.velaTransferHook}");`, "transfer hook declare_id");
assertIncludes(
  resolve(protocolRoot, "programs/vela-protocol/src/constants.rs"),
  `pub const TRANSFER_HOOK_PROGRAM_ID_BYTES: [u8; 32] = [${hookBytes}];`,
  "transfer hook bytes",
);
assertIncludes(resolve(protocolRoot, "scripts/deploy-devnet.sh"), 'ids["velaProtocol"]', "deploy script protocol manifest lookup");
assertIncludes(resolve(protocolRoot, "scripts/deploy-devnet.sh"), 'ids["velaTransferHook"]', "deploy script transfer hook manifest lookup");
assertIncludes(resolve(protocolRoot, "ts-tests/setup.ts"), "transferHookIdl.address", "ts test hook id");
assertIncludes(resolve(protocolRoot, "README.md"), `| \`vela-protocol\` | \`${devnet.velaProtocol}\` |`, "README protocol id");
assertIncludes(
  resolve(protocolRoot, "README.md"),
  `| \`vela-transfer-hook\` | \`${devnet.velaTransferHook}\` |`,
  "README transfer hook id",
);

console.log("Program ID manifest and protocol repo sources are in sync.");
