#!/usr/bin/env node
const { execFileSync } = require("child_process");
const { existsSync, readFileSync } = require("fs");
const { homedir } = require("os");
const { resolve } = require("path");
const { deflateSync, inflateSync } = require("zlib");
const {
  ComputeBudgetProgram,
  Connection,
  Keypair,
  PublicKey,
  sendAndConfirmTransaction,
  Transaction,
  TransactionInstruction,
} = require("@solana/web3.js");

const PROGRAM_METADATA_PROGRAM_ID = new PublicKey(
  "ProgM6JCCvbYkfKqJYHePx4xxSUSqJp7rh8Lyv7nk7S",
);
const HEADER_LENGTH = 96;
const IDL_SEED = Buffer.concat([Buffer.from("idl"), Buffer.alloc(13)]);
const CHUNK_SIZE = Number(process.env.PMP_IDL_CHUNK_SIZE || 700);
const MAX_PASSES = Number(process.env.PMP_IDL_MAX_PASSES || 3);
const PRIORITY_FEE = Number(process.env.PRIORITY_FEE || 100000);

function usage() {
  console.error("Usage: node scripts/sync-pmp-idl.cjs <program-id> <idl-json>");
  process.exit(2);
}

function sleep(ms) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms));
}

function readKeypair(path) {
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(path, "utf8"))));
}

function metadataAddress(programId) {
  return PublicKey.findProgramAddressSync(
    [new PublicKey(programId).toBuffer(), IDL_SEED],
    PROGRAM_METADATA_PROGRAM_ID,
  )[0];
}

function writeInstruction(metadata, authority, offset, data) {
  const prefix = Buffer.alloc(5);
  prefix.writeUInt8(0, 0);
  prefix.writeUInt32LE(offset, 1);

  return new TransactionInstruction({
    programId: PROGRAM_METADATA_PROGRAM_ID,
    keys: [
      { pubkey: metadata, isSigner: false, isWritable: true },
      { pubkey: authority.publicKey, isSigner: true, isWritable: false },
      { pubkey: PROGRAM_METADATA_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.concat([prefix, data]),
  });
}

async function fetchAccount(connection, address) {
  return await connection.getAccountInfo(address, "confirmed");
}

function accountNeedsBootstrap(account, expectedLength) {
  return (
    !account ||
    !account.owner.equals(PROGRAM_METADATA_PROGRAM_ID) ||
    account.data.length !== HEADER_LENGTH + expectedLength
  );
}

function runProgramMetadataWrite(rpcUrl, keypairPath, programId, idlPath) {
  const args = [
    "exec",
    "--yes",
    "@solana-program/program-metadata@0.7.0",
    "--",
    "--rpc",
    rpcUrl,
    "--keypair",
    keypairPath,
    "--priority-fees",
    String(PRIORITY_FEE),
    "write",
    "idl",
    programId,
    idlPath,
  ];

  try {
    execFileSync("npm", args, { stdio: "inherit" });
  } catch (error) {
    console.warn(
      "WARN: program-metadata write did not complete cleanly; attempting chunk repair.",
    );
  }
}

async function sendWrite(connection, metadata, authority, offset, chunk) {
  for (let attempt = 1; attempt <= 4; attempt += 1) {
    try {
      const tx = new Transaction().add(
        ComputeBudgetProgram.setComputeUnitPrice({ microLamports: PRIORITY_FEE }),
        writeInstruction(metadata, authority, offset, chunk),
      );
      return await sendAndConfirmTransaction(connection, tx, [authority], {
        commitment: "confirmed",
        maxRetries: 5,
        skipPreflight: false,
      });
    } catch (error) {
      if (attempt === 4) throw error;
      await sleep(750 * attempt);
    }
  }
}

function firstDiff(actual, expected) {
  for (let i = 0; i < expected.length; i += 1) {
    if (actual[i] !== expected[i]) return i;
  }
  return -1;
}

async function main() {
  const [programId, idlPathArg] = process.argv.slice(2);
  if (!programId || !idlPathArg) usage();

  const idlPath = resolve(idlPathArg);
  if (!existsSync(idlPath)) {
    throw new Error(`IDL file does not exist: ${idlPath}`);
  }

  const rpcUrl = process.env.RPC_URL || "https://api.devnet.solana.com";
  const keypairPath =
    process.env.ANCHOR_PROVIDER_WALLET ||
    process.env.PROVIDER_WALLET ||
    process.env.KEYPAIR ||
    `${homedir()}/.config/solana/id.json`;
  const authority = readKeypair(keypairPath);
  const connection = new Connection(rpcUrl, "confirmed");
  const metadata = metadataAddress(programId);
  const compressed = deflateSync(readFileSync(idlPath));

  console.log(`==> Syncing PMP IDL for ${programId}`);
  console.log(`    metadata: ${metadata.toBase58()}`);
  console.log(`    compressed bytes: ${compressed.length}`);

  let account = await fetchAccount(connection, metadata);
  if (accountNeedsBootstrap(account, compressed.length)) {
    runProgramMetadataWrite(rpcUrl, keypairPath, programId, idlPath);
    await sleep(2500);
    account = await fetchAccount(connection, metadata);
  }

  if (accountNeedsBootstrap(account, compressed.length)) {
    throw new Error(
      `PMP metadata account is missing or incorrectly sized after bootstrap: ${metadata.toBase58()}`,
    );
  }

  for (let pass = 1; pass <= MAX_PASSES; pass += 1) {
    const payload = account.data.subarray(HEADER_LENGTH, HEADER_LENGTH + compressed.length);
    const diff = firstDiff(payload, compressed);
    if (diff < 0) break;

    let writes = 0;
    for (let offset = 0; offset < compressed.length; offset += CHUNK_SIZE) {
      const chunk = compressed.subarray(offset, Math.min(offset + CHUNK_SIZE, compressed.length));
      const current = payload.subarray(offset, offset + chunk.length);
      if (Buffer.compare(Buffer.from(current), chunk) === 0) continue;

      const signature = await sendWrite(connection, metadata, authority, offset, chunk);
      writes += 1;
      console.log(`    wrote offset ${offset} len ${chunk.length}: ${signature}`);
      await sleep(300);
    }

    console.log(`    repair pass ${pass}: ${writes} write transaction(s)`);
    await sleep(1000);
    account = await fetchAccount(connection, metadata);
  }

  const payload = account.data.subarray(HEADER_LENGTH, HEADER_LENGTH + compressed.length);
  const diff = firstDiff(payload, compressed);
  if (diff >= 0) {
    throw new Error(`PMP IDL payload still differs at compressed byte ${diff}`);
  }

  JSON.parse(inflateSync(payload).toString("utf8"));
  console.log("    PASS PMP IDL payload matches local IDL");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
