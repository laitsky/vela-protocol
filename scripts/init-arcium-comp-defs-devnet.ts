/* biome-ignore-all lint/suspicious/noExplicitAny: Anchor program methods are IDL-generated. */

import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { AnchorProvider, BN, Program, Wallet } from "@coral-xyz/anchor";
import {
  AddressLookupTableProgram,
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import {
  getArciumProgram,
  getArciumProgramId,
  getCompDefAccAddress,
  getLookupTableAddress,
  getMXEAccAddress,
  getRawCircuitAccAddress,
} from "@arcium-hq/client";

import { loadProgramIds, protocolRoot } from "./program-ids";

type CircuitName =
  | "validate_mandate"
  | "usage_charge"
  | "tiered_pricing"
  | "record_billing_event";
type CompDefName =
  | "validate_mandate_v2"
  | "usage_charge_v2"
  | "tiered_pricing_v2"
  | "record_billing_event_v2";

interface CircuitConfig {
  rawName: CircuitName;
  compDefName: CompDefName;
  method: string;
}

const CIRCUITS: CircuitConfig[] = [
  {
    rawName: "validate_mandate",
    compDefName: "validate_mandate_v2",
    method: "initValidateMandateCompDef",
  },
  {
    rawName: "usage_charge",
    compDefName: "usage_charge_v2",
    method: "initUsageChargeCompDef",
  },
  {
    rawName: "tiered_pricing",
    compDefName: "tiered_pricing_v2",
    method: "initTieredPricingCompDef",
  },
  {
    rawName: "record_billing_event",
    compDefName: "record_billing_event_v2",
    method: "initRecordBillingCompDef",
  },
];

const RPC_URL = process.env.RPC_URL ?? process.env.SOLANA_RPC_URL ?? "https://api.devnet.solana.com";
const WALLET_PATH =
  process.env.ANCHOR_WALLET ??
  join(process.env.HOME ?? "", ".config", "solana", "id.json");
const CHUNK_SIZE = Number(process.env.ARCIUM_UPLOAD_CHUNK_SIZE ?? 8);
const UPLOAD_DELAY_MS = Number(process.env.ARCIUM_UPLOAD_DELAY_MS ?? 500);
const RAW_CIRCUIT_HEADER_SIZE = 9;
const MAX_RAW_CIRCUIT_DATA_BYTES = 10_485_760 - RAW_CIRCUIT_HEADER_SIZE;
const MAX_REALLOC_PER_IX = 10_240;
const MAX_EMBIGGEN_IX_PER_TX = 18;
const MAX_UPLOAD_PER_TX_BYTES = 814;
const DEFAULT_PUBLIC_KEY = new PublicKey("11111111111111111111111111111111");
const REQUIRE_MXE_KEYS = process.env.ARCIUM_REQUIRE_MXE_KEYS !== "0";

if (!Number.isInteger(CHUNK_SIZE) || CHUNK_SIZE < 1) {
  throw new Error(`ARCIUM_UPLOAD_CHUNK_SIZE must be a positive integer, got ${CHUNK_SIZE}`);
}
if (!Number.isInteger(UPLOAD_DELAY_MS) || UPLOAD_DELAY_MS < 0) {
  throw new Error(`ARCIUM_UPLOAD_DELAY_MS must be a non-negative integer, got ${UPLOAD_DELAY_MS}`);
}

function loadKeypair(path: string): Keypair {
  if (!existsSync(path)) {
    throw new Error(`Wallet keypair not found at ${path}`);
  }
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(path, "utf8"))));
}

function compDefOffset(circuitName: string): number {
  return createHash("sha256").update(circuitName).digest().readUInt32LE(0);
}

function readCircuit(circuitName: string): Uint8Array {
  const path = resolve(protocolRoot, "build", `${circuitName}.arcis`);
  if (!existsSync(path)) {
    throw new Error(`Missing ${path}; run arcium build first`);
  }
  return readFileSync(path);
}

function protocolConfigAddress(programId: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([Buffer.from("config")], programId)[0];
}

function assertMxeKeysReady(mxeProgramId: PublicKey) {
  if (!REQUIRE_MXE_KEYS) {
    console.log("MXE key finalization preflight skipped by ARCIUM_REQUIRE_MXE_KEYS=0");
    return;
  }

  const output = execFileSync("arcium", ["mxe-keys", mxeProgramId.toBase58(), "-u", "devnet"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (output.includes("MXE keys not yet set")) {
    throw new Error(
      `MXE keys are not finalized for ${mxeProgramId.toBase58()}. ` +
        "Refusing to initialize/upload comp defs because callbacks would remain unavailable.",
    );
  }
}

async function resolveMxeProgramId(velaProgram: Program, programId: PublicKey): Promise<PublicKey> {
  if (process.env.ARCIUM_MXE_PROGRAM_ID) {
    return new PublicKey(process.env.ARCIUM_MXE_PROGRAM_ID);
  }

  const config = (await (velaProgram.account as any).protocolConfig.fetch(
    protocolConfigAddress(programId),
  )) as { mxeProgramId?: PublicKey };
  if (!config.mxeProgramId || config.mxeProgramId.equals(DEFAULT_PUBLIC_KEY)) {
    return programId;
  }
  return config.mxeProgramId;
}

function stringifyForLogs(value: unknown): string {
  return JSON.stringify(
    value,
    (_key, item) => {
      if (BN.isBN(item)) return item.toString();
      if (item instanceof PublicKey) return item.toBase58();
      return item;
    },
    2,
  );
}

async function assertExistingRawCircuitBytesMatch(
  connection: Connection,
  compDefAccount: PublicKey,
  rawCircuit: Uint8Array,
  requireExisting: boolean,
): Promise<void> {
  const rawAccountCount = Math.ceil(rawCircuit.length / MAX_RAW_CIRCUIT_DATA_BYTES);

  for (let index = 0; index < rawAccountCount; index++) {
    const start = index * MAX_RAW_CIRCUIT_DATA_BYTES;
    const end = Math.min(start + MAX_RAW_CIRCUIT_DATA_BYTES, rawCircuit.length);
    const expected = Buffer.from(rawCircuit.subarray(start, end));
    const rawCircuitAccount = getRawCircuitAccAddress(compDefAccount, index);
    const accountInfo = await connection.getAccountInfo(rawCircuitAccount, "confirmed");

    if (!accountInfo || accountInfo.data.length < expected.length + RAW_CIRCUIT_HEADER_SIZE) {
      if (requireExisting) {
        throw new Error(
          `Existing finalized comp definition ${compDefAccount.toBase58()} is missing raw circuit ` +
            `account ${rawCircuitAccount.toBase58()} or the account is too small to verify.`,
        );
      }
      continue;
    }

    const actual = accountInfo.data.subarray(
      RAW_CIRCUIT_HEADER_SIZE,
      RAW_CIRCUIT_HEADER_SIZE + expected.length,
    );
    if (!actual.equals(expected)) {
      if (!requireExisting) {
        continue;
      }
      throw new Error(
        `Existing raw circuit account ${rawCircuitAccount.toBase58()} has sufficient size but ` +
          `does not match local circuit bytes. Refusing to finalize a possibly partial upload.`,
      );
    }
  }
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, ms));
}

async function sendWithRetry(
  provider: AnchorProvider,
  tx: Transaction,
  label: string,
): Promise<string> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= 8; attempt++) {
    try {
      const blockhash = await provider.connection.getLatestBlockhash("confirmed");
      tx.recentBlockhash = blockhash.blockhash;
      tx.lastValidBlockHeight = blockhash.lastValidBlockHeight;
      return await provider.sendAndConfirm(tx, [], {
        commitment: "confirmed",
        preflightCommitment: "confirmed",
      });
    } catch (error) {
      lastError = error;
      const message = error instanceof Error ? error.message : String(error);
      const retryable =
        message.includes("429") ||
        message.includes("Too Many Requests") ||
        message.includes("block height exceeded") ||
        message.includes("Blockhash not found") ||
        message.includes("Transaction was not confirmed");
      if (!retryable || attempt === 8) {
        throw error;
      }
      const delayMs = Math.min(30_000, 750 * 2 ** (attempt - 1));
      console.warn(`  ${label} failed on attempt ${attempt}: ${message}`);
      console.warn(`  retrying in ${delayMs}ms`);
      await sleep(delayMs);
    }
  }
  throw lastError;
}

async function sendRawWithRetry(
  provider: AnchorProvider,
  tx: Transaction,
  label: string,
  sharedBlockhash?: Awaited<ReturnType<Connection["getLatestBlockhash"]>>,
): Promise<string> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= 8; attempt++) {
    try {
      const blockhash =
        attempt === 1 && sharedBlockhash
          ? sharedBlockhash
          : await provider.connection.getLatestBlockhash("confirmed");
      tx.feePayer = provider.publicKey;
      tx.recentBlockhash = blockhash.blockhash;
      tx.lastValidBlockHeight = blockhash.lastValidBlockHeight;
      const signed = await provider.wallet.signTransaction(tx);
      return await provider.connection.sendRawTransaction(signed.serialize(), {
        maxRetries: 8,
        skipPreflight: true,
      });
    } catch (error) {
      lastError = error;
      const message = error instanceof Error ? error.message : String(error);
      const retryable =
        message.includes("429") ||
        message.includes("Too Many Requests") ||
        message.includes("block height exceeded") ||
        message.includes("Blockhash not found");
      if (!retryable || attempt === 8) {
        throw error;
      }
      const delayMs = Math.min(20_000, 500 * 2 ** (attempt - 1));
      console.warn(`  ${label} raw send failed on attempt ${attempt}: ${message}`);
      console.warn(`  retrying in ${delayMs}ms`);
      await sleep(delayMs);
    }
  }
  throw lastError;
}

function paddedUploadBytes(rawCircuitPart: Uint8Array, offset: number): number[] {
  const chunk = rawCircuitPart.subarray(offset, offset + MAX_UPLOAD_PER_TX_BYTES);
  const padded = Buffer.alloc(MAX_UPLOAD_PER_TX_BYTES);
  padded.set(chunk);
  return Array.from(padded);
}

async function ensureRawCircuitAccount(
  provider: AnchorProvider,
  arciumProgram: ReturnType<typeof getArciumProgram>,
  compDefOffsetValue: number,
  mxeProgramId: PublicKey,
  rawCircuitIndex: number,
  requiredAccountSize: number,
): Promise<void> {
  const rawCircuitAccount = getRawCircuitAccAddress(
    getCompDefAccAddress(mxeProgramId, compDefOffsetValue),
    rawCircuitIndex,
  );
  let accountInfo = await provider.connection.getAccountInfo(rawCircuitAccount, "confirmed");

  if (!accountInfo) {
    const signature = await (arciumProgram.methods as any)
      .initRawCircuitAcc(compDefOffsetValue, mxeProgramId, rawCircuitIndex)
      .accounts({ signer: provider.publicKey })
      .rpc();
    console.log(`  raw circuit ${rawCircuitIndex} initialized: ${signature}`);
    accountInfo = await provider.connection.getAccountInfo(rawCircuitAccount, "confirmed");
  }

  let currentSize = accountInfo?.data.length ?? 0;
  while (currentSize < requiredAccountSize) {
    const resizeSize = Math.min(
      requiredAccountSize - currentSize,
      MAX_REALLOC_PER_IX * MAX_EMBIGGEN_IX_PER_TX,
    );
    const resizeIxCount = Math.ceil(resizeSize / MAX_REALLOC_PER_IX);
    const tx = new Transaction();
    for (let i = 0; i < resizeIxCount; i++) {
      tx.add(
        await (arciumProgram.methods as any)
          .embiggenRawCircuitAcc(compDefOffsetValue, mxeProgramId, rawCircuitIndex)
          .accounts({ signer: provider.publicKey })
          .instruction(),
      );
    }
    await sendWithRetry(provider, tx, `resize raw circuit ${rawCircuitIndex}`);
    currentSize += resizeIxCount * MAX_REALLOC_PER_IX;
    console.log(`  raw circuit ${rawCircuitIndex} resized to at least ${currentSize} bytes`);
  }
}

async function findMismatchedUploadIndexes(
  connection: Connection,
  compDefAccount: PublicKey,
  rawCircuitIndex: number,
  rawCircuitPart: Uint8Array,
): Promise<number[]> {
  const rawCircuitAccount = getRawCircuitAccAddress(compDefAccount, rawCircuitIndex);
  const accountInfo = await connection.getAccountInfo(rawCircuitAccount, "confirmed");
  const uploadTxCount = Math.ceil(rawCircuitPart.length / MAX_UPLOAD_PER_TX_BYTES);
  if (!accountInfo || accountInfo.data.length < rawCircuitPart.length + RAW_CIRCUIT_HEADER_SIZE) {
    return Array.from({ length: uploadTxCount }, (_value, index) => index);
  }

  const mismatches: number[] = [];
  for (let uploadIndex = 0; uploadIndex < uploadTxCount; uploadIndex++) {
    const start = uploadIndex * MAX_UPLOAD_PER_TX_BYTES;
    const end = Math.min(start + MAX_UPLOAD_PER_TX_BYTES, rawCircuitPart.length);
    const expected = rawCircuitPart.subarray(start, end);
    const actual = accountInfo.data.subarray(
      RAW_CIRCUIT_HEADER_SIZE + start,
      RAW_CIRCUIT_HEADER_SIZE + end,
    );
    if (!actual.equals(Buffer.from(expected))) {
      mismatches.push(uploadIndex);
    }
  }
  return mismatches;
}

async function uploadCircuitDeterministically(
  provider: AnchorProvider,
  arciumProgram: ReturnType<typeof getArciumProgram>,
  circuitName: CompDefName,
  mxeProgramId: PublicKey,
  compDefAccount: PublicKey,
  compDefOffsetValue: number,
  rawCircuit: Uint8Array,
): Promise<string[]> {
  const beforeUpload = await (arciumProgram.account as any).computationDefinitionAccount.fetch(
    compDefAccount,
  );
  if (!beforeUpload.circuitSource.onChain?.[0]) {
    throw new Error(`${circuitName} comp definition is not configured for on-chain upload`);
  }
  if (beforeUpload.circuitSource.onChain[0].isCompleted) {
    console.log("  upload skipped: finalized");
    return [];
  }

  const signatures: string[] = [];
  const rawAccountCount = Math.ceil(rawCircuit.length / MAX_RAW_CIRCUIT_DATA_BYTES);

  for (let rawCircuitIndex = 0; rawCircuitIndex < rawAccountCount; rawCircuitIndex++) {
    const start = rawCircuitIndex * MAX_RAW_CIRCUIT_DATA_BYTES;
    const end = Math.min(start + MAX_RAW_CIRCUIT_DATA_BYTES, rawCircuit.length);
    const rawCircuitPart = rawCircuit.subarray(start, end);
    await ensureRawCircuitAccount(
      provider,
      arciumProgram,
      compDefOffsetValue,
      mxeProgramId,
      rawCircuitIndex,
      rawCircuitPart.length + RAW_CIRCUIT_HEADER_SIZE,
    );

    const uploadTxCount = Math.ceil(rawCircuitPart.length / MAX_UPLOAD_PER_TX_BYTES);
    for (let round = 1; round <= 8; round++) {
      const missingIndexes = await findMismatchedUploadIndexes(
        provider.connection,
        compDefAccount,
        rawCircuitIndex,
        rawCircuitPart,
      );
      if (missingIndexes.length === 0) {
        console.log(`  raw circuit ${rawCircuitIndex}: bytes verified`);
        break;
      }
      console.log(
        `  raw circuit ${rawCircuitIndex}: upload round ${round}, ${missingIndexes.length}/${uploadTxCount} chunks need writes`,
      );
      let sharedBlockhash = await provider.connection.getLatestBlockhash("confirmed");
      let sharedBlockhashUses = 0;
      for (let cursor = 0; cursor < missingIndexes.length; cursor += CHUNK_SIZE) {
        const chunk = missingIndexes.slice(cursor, cursor + CHUNK_SIZE);
        if (sharedBlockhashUses + chunk.length > 30) {
          sharedBlockhash = await provider.connection.getLatestBlockhash("confirmed");
          sharedBlockhashUses = 0;
        }
        const chunkPromises = chunk.map(async (uploadIndex) => {
          const offset = uploadIndex * MAX_UPLOAD_PER_TX_BYTES;
          const tx = await (arciumProgram.methods as any)
            .uploadCircuit(
              compDefOffsetValue,
              mxeProgramId,
              rawCircuitIndex,
              paddedUploadBytes(rawCircuitPart, offset),
              offset,
            )
            .accounts({ signer: provider.publicKey })
            .transaction();
          return sendRawWithRetry(
            provider,
            tx,
            `upload ${circuitName} raw ${rawCircuitIndex} tx ${uploadIndex}`,
            sharedBlockhash,
          );
        });
        signatures.push(...(await Promise.all(chunkPromises)));
        sharedBlockhashUses += chunk.length;
        if ((cursor / CHUNK_SIZE + 1) % 25 === 0 || cursor + CHUNK_SIZE >= missingIndexes.length) {
          console.log(
            `  raw circuit ${rawCircuitIndex}: sent ${Math.min(cursor + CHUNK_SIZE, missingIndexes.length)} of ${missingIndexes.length} missing chunks`,
          );
        }
        await sleep(UPLOAD_DELAY_MS);
      }
      await sleep(2_500);
      if (round === 8) {
        const remaining = await findMismatchedUploadIndexes(
          provider.connection,
          compDefAccount,
          rawCircuitIndex,
          rawCircuitPart,
        );
        if (remaining.length > 0) {
          throw new Error(
            `${circuitName} raw circuit ${rawCircuitIndex} still has ${remaining.length} mismatched chunks after upload retries`,
          );
        }
      }
    }
  }

  await assertExistingRawCircuitBytesMatch(provider.connection, compDefAccount, rawCircuit, true);

  const finalizeTx = await (arciumProgram.methods as any)
    .finalizeComputationDefinition(compDefOffsetValue, mxeProgramId)
    .accounts({ signer: provider.publicKey })
    .transaction();
  signatures.push(await sendWithRetry(provider, finalizeTx, `finalize ${circuitName}`));
  return signatures;
}

async function main() {
  const payer = loadKeypair(WALLET_PATH);
  const wallet = new Wallet(payer);
  const connection = new Connection(RPC_URL, "confirmed");
  const provider = new AnchorProvider(connection, wallet, {
    commitment: "confirmed",
    preflightCommitment: "confirmed",
  });
  const manifest = loadProgramIds();
  const programId = new PublicKey(manifest.devnet.velaProtocol);
  const arciumProgramId = getArciumProgramId();
  const velaIdl = JSON.parse(
    readFileSync(resolve(protocolRoot, "target/idl/vela_protocol.json"), "utf8"),
  );
  const velaProgram = new Program(velaIdl, provider) as Program;
  const arciumProgram = getArciumProgram(provider);
  const mxeProgramId = await resolveMxeProgramId(velaProgram, programId);

  const mxeAccount = getMXEAccAddress(mxeProgramId);
  const mxe = await arciumProgram.account.mxeAccount.fetch(mxeAccount);
  const addressLookupTable = getLookupTableAddress(mxeProgramId, mxe.lutOffsetSlot);

  console.log(`RPC: ${RPC_URL}`);
  console.log(`payer: ${payer.publicKey.toBase58()}`);
  console.log(`vela protocol program: ${programId.toBase58()}`);
  console.log(`configured MXE program: ${mxeProgramId.toBase58()}`);
  console.log(`Arcium program: ${arciumProgramId.toBase58()}`);
  console.log(`MXE account: ${mxeAccount.toBase58()}`);
  console.log(`MXE LUT slot: ${mxe.lutOffsetSlot.toString()}`);
  console.log(`MXE LUT: ${addressLookupTable.toBase58()}`);
  assertMxeKeysReady(mxeProgramId);

  const lutInfo = await connection.getAccountInfo(addressLookupTable, "confirmed");
  if (!lutInfo) {
    throw new Error(`MXE lookup table account does not exist: ${addressLookupTable.toBase58()}`);
  }

  for (const circuit of CIRCUITS) {
    const offset = compDefOffset(circuit.compDefName);
    const compDefAccount = getCompDefAccAddress(mxeProgramId, offset);
    const existing = await connection.getAccountInfo(compDefAccount, "confirmed");

    console.log(`\n${circuit.compDefName} (${circuit.rawName})`);
    console.log(`  offset: ${offset}`);
    console.log(`  comp def: ${compDefAccount.toBase58()}`);

    if (!existing) {
      const signature = await (velaProgram.methods as any)[circuit.method]()
        .accounts({
          payer: payer.publicKey,
          mxeProgram: mxeProgramId,
          mxeAccount,
          compDefAccount,
          addressLookupTable,
          lutProgram: AddressLookupTableProgram.programId,
          arciumProgram: arciumProgramId,
          systemProgram: SystemProgram.programId,
        })
        .rpc();
      console.log(`  initialized: ${signature}`);
    } else {
      console.log("  already initialized");
    }

    const beforeUpload = await arciumProgram.account.computationDefinitionAccount.fetch(
      compDefAccount,
    );
    console.log(`  state before upload: ${stringifyForLogs(beforeUpload.circuitSource)}`);

    const rawCircuit = readCircuit(circuit.rawName);
    const isFinalized = Boolean(beforeUpload.circuitSource.onChain?.[0]?.isCompleted);
    await assertExistingRawCircuitBytesMatch(connection, compDefAccount, rawCircuit, isFinalized);

    const signatures = await uploadCircuitDeterministically(
      provider,
      arciumProgram,
      circuit.compDefName,
      mxeProgramId,
      compDefAccount,
      offset,
      rawCircuit,
    );
    if (signatures.length === 0) {
      console.log("  upload skipped");
    } else {
      console.log(
        `  upload/finalize txs: ${signatures.length} sent ` +
          `(first ${signatures[0]}, last ${signatures[signatures.length - 1]})`,
      );
    }

    const afterUpload = await arciumProgram.account.computationDefinitionAccount.fetch(
      compDefAccount,
    );
    console.log(`  state after upload: ${stringifyForLogs(afterUpload.circuitSource)}`);
    if (!afterUpload.circuitSource.onChain?.[0]?.isCompleted) {
      throw new Error(`${circuit.name} comp definition is not finalized after upload`);
    }
  }
}

await main();
