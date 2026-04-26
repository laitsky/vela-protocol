import { readFileSync } from "node:fs";
import { resolve } from "node:path";

type ClusterName = "devnet" | "localnet";

type ProgramIds = {
  velaProtocol: string;
  velaTransferHook: string;
};

type ProgramIdsManifest = {
  defaultCluster: ClusterName;
  devnet: ProgramIds;
  localnet: ProgramIds;
};

export const protocolRoot = resolve(import.meta.dir, "..");
const manifestPath = resolve(protocolRoot, "config/program-ids.json");

export function loadProgramIds(): ProgramIdsManifest {
  return JSON.parse(readFileSync(manifestPath, "utf8")) as ProgramIdsManifest;
}

export function bytesForBase58(pubkey: string): number[] {
  const alphabet = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
  const alphabetMap = new Map(
    alphabet.split("").map((char, index) => [char, index]),
  );

  let value = 0n;
  for (const char of pubkey) {
    const digit = alphabetMap.get(char);
    if (digit === undefined) {
      throw new Error(`Invalid base58 character "${char}" in ${pubkey}`);
    }
    value = value * 58n + BigInt(digit);
  }

  let hex = value.toString(16);
  if (hex.length % 2 !== 0) {
    hex = `0${hex}`;
  }

  const bytes = hex === "" ? [] : [...Buffer.from(hex, "hex")];
  let leadingZeroCount = 0;
  for (const char of pubkey) {
    if (char === "1") {
      leadingZeroCount += 1;
    } else {
      break;
    }
  }

  const fullBytes = [...new Array(leadingZeroCount).fill(0), ...bytes];
  if (fullBytes.length !== 32) {
    throw new Error(
      `Expected 32 decoded bytes for ${pubkey}, got ${fullBytes.length}`,
    );
  }

  return fullBytes;
}
export function replaceExact(
  content: string,
  before: string,
  after: string,
  filePath: string,
): string {
  if (!content.includes(before)) {
    throw new Error(`Expected snippet not found in ${filePath}`);
  }
  return content.replace(before, after);
}

export function replacePattern(
  content: string,
  pattern: RegExp,
  replacer: string | ((substring: string, ...args: string[]) => string),
  filePath: string,
): string {
  if (!pattern.test(content)) {
    throw new Error(`Expected pattern not found in ${filePath}`);
  }
  pattern.lastIndex = 0;
  return content.replace(pattern, replacer as never);
}
