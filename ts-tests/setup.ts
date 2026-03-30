import { BN, Program, Wallet } from "@coral-xyz/anchor";
import { LiteSVMProvider } from "anchor-litesvm";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  ACCOUNT_SIZE,
  MINT_SIZE,
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createInitializeMint2Instruction,
  createMintToInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SYSVAR_RENT_PUBKEY,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import { LiteSVM } from "anchor-litesvm/node_modules/litesvm";
import { existsSync } from "node:fs";
import { join } from "node:path";
import idl from "../target/idl/vela_protocol.json";

const PROGRAM_ID = new PublicKey(idl.address);
const PROGRAM_SO_PATH = join(process.cwd(), "target", "deploy", "vela_protocol.so");
const DECIMALS = 6;

export type VelaProgram = Program<any>;

export type PlanAddresses = {
  merchantState: PublicKey;
  plan: PublicKey;
  credentialMint: PublicKey;
};

export type TestContext = {
  svm: LiteSVM;
  provider: LiteSVMProvider;
  program: VelaProgram;
  programId: PublicKey;
  merchant: Keypair;
};

export type SubscriptionFixture = TestContext & {
  subscriber: Keypair;
  planAddresses: PlanAddresses;
  mandate: PublicKey;
  credentialAta: PublicKey;
  usdcMint: PublicKey;
  subscriberTokenAccount: PublicKey;
  merchantTokenAccount: PublicKey;
  amount: bigint;
  frequency: bigint;
  maxPulls: bigint;
};

export function setupTest(): TestContext {
  if (!existsSync(PROGRAM_SO_PATH)) {
    throw new Error(`Expected compiled program at ${PROGRAM_SO_PATH}`);
  }

  const svm = new LiteSVM().withDefaultPrograms().withTransactionHistory(0n);
  svm.addProgramFromFile(PROGRAM_ID, PROGRAM_SO_PATH);

  const merchant = Keypair.generate();
  airdropSol(svm, merchant.publicKey, 10n * BigInt(LAMPORTS_PER_SOL));

  const provider = new LiteSVMProvider(svm, new Wallet(merchant));
  const program = new Program(idl as never, provider) as Program<any>;

  return { svm, provider, program, programId: PROGRAM_ID, merchant };
}

export function derivePlanAddresses(
  merchant: PublicKey,
  planId: bigint,
  programId = PROGRAM_ID,
): PlanAddresses {
  const planIdSeed = new BN(planId.toString()).toArrayLike(Buffer, "le", 8);
  const [merchantState] = PublicKey.findProgramAddressSync(
    [Buffer.from("merchant"), merchant.toBuffer()],
    programId,
  );
  const [plan] = PublicKey.findProgramAddressSync(
    [Buffer.from("plan"), merchant.toBuffer(), planIdSeed],
    programId,
  );
  const [credentialMint] = PublicKey.findProgramAddressSync(
    [Buffer.from("credential"), merchant.toBuffer(), planIdSeed],
    programId,
  );

  return { merchantState, plan, credentialMint };
}

export function deriveMandateAddress(
  subscriber: PublicKey,
  plan: PublicKey,
  programId = PROGRAM_ID,
): PublicKey {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("mandate"), subscriber.toBuffer(), plan.toBuffer()],
    programId,
  )[0];
}

export function airdropSol(
  svm: LiteSVM,
  pubkey: PublicKey,
  lamports = BigInt(LAMPORTS_PER_SOL),
): void {
  svm.airdrop(pubkey, lamports);
}

export function advanceClock(svm: LiteSVM, unixTimestamp: bigint): void {
  const clock = svm.getClock();
  clock.unixTimestamp = unixTimestamp;
  svm.setClock(clock);
}

export async function sendInstructions(
  provider: LiteSVMProvider,
  instructions: Parameters<Transaction["add"]>,
  signers: Keypair[] = [],
): Promise<string> {
  provider.client.expireBlockhash();
  const tx = new Transaction().add(...instructions);
  return provider.sendAndConfirm!(tx, signers);
}

export async function createUsdcMint(
  provider: LiteSVMProvider,
  mintAuthority = provider.wallet.publicKey,
): Promise<PublicKey> {
  const mint = Keypair.generate();
  const lamports =
    await provider.connection.getMinimumBalanceForRentExemption(MINT_SIZE);

  await sendInstructions(
    provider,
    [
      SystemProgram.createAccount({
        fromPubkey: provider.wallet.publicKey,
        newAccountPubkey: mint.publicKey,
        lamports,
        space: MINT_SIZE,
        programId: TOKEN_PROGRAM_ID,
      }),
      createInitializeMint2Instruction(
        mint.publicKey,
        DECIMALS,
        mintAuthority,
        null,
        TOKEN_PROGRAM_ID,
      ),
    ],
    [mint],
  );

  return mint.publicKey;
}

export async function createSplTokenAccount(
  provider: LiteSVMProvider,
  owner: PublicKey,
  mint: PublicKey,
): Promise<PublicKey> {
  const ata = getAssociatedTokenAddressSync(mint, owner, false, TOKEN_PROGRAM_ID);
  await sendInstructions(provider, [
    createAssociatedTokenAccountIdempotentInstruction(
      provider.wallet.publicKey,
      ata,
      owner,
      mint,
      TOKEN_PROGRAM_ID,
    ),
  ]);
  return ata;
}

export async function mintUsdc(
  provider: LiteSVMProvider,
  mint: PublicKey,
  destination: PublicKey,
  amount: bigint,
): Promise<string> {
  return sendInstructions(provider, [
    createMintToInstruction(
      mint,
      destination,
      provider.wallet.publicKey,
      amount,
      [],
      TOKEN_PROGRAM_ID,
    ),
  ]);
}

export async function createPlan(
  context: TestContext,
  amount: bigint,
  frequency: bigint,
  trialPeriod: bigint,
  maxPulls: bigint,
  planId = 0n,
): Promise<PlanAddresses> {
  const addresses = derivePlanAddresses(context.merchant.publicKey, planId, context.programId);
  context.svm.expireBlockhash();
  await (context.program as any).methods
    .createPlan(
      new BN(amount.toString()),
      new BN(frequency.toString()),
      new BN(trialPeriod.toString()),
      new BN(maxPulls.toString()),
    )
    .accounts({
      merchant: context.merchant.publicKey,
      merchantState: addresses.merchantState,
      plan: addresses.plan,
      credentialMint: addresses.credentialMint,
      systemProgram: SystemProgram.programId,
      token2022Program: TOKEN_2022_PROGRAM_ID,
      rent: SYSVAR_RENT_PUBKEY,
    })
    .rpc();

  return addresses;
}

export async function createSubscriptionFixture(options?: {
  amount?: bigint;
  frequency?: bigint;
  trialPeriod?: bigint;
  maxPulls?: bigint;
  planId?: bigint;
}): Promise<SubscriptionFixture> {
  const amount = options?.amount ?? 25_000_000n;
  const frequency = options?.frequency ?? 3_600n;
  const trialPeriod = options?.trialPeriod ?? 0n;
  const maxPulls = options?.maxPulls ?? 4n;
  const planId = options?.planId ?? 0n;

  const context = setupTest();
  const subscriber = Keypair.generate();
  airdropSol(context.svm, subscriber.publicKey, 5n * BigInt(LAMPORTS_PER_SOL));

  const planAddresses = await createPlan(
    context,
    amount,
    frequency,
    trialPeriod,
    maxPulls,
    planId,
  );
  const usdcMint = await createUsdcMint(context.provider);
  const subscriberTokenAccount = await createSplTokenAccount(
    context.provider,
    subscriber.publicKey,
    usdcMint,
  );
  const merchantTokenAccount = await createSplTokenAccount(
    context.provider,
    context.merchant.publicKey,
    usdcMint,
  );
  await mintUsdc(
    context.provider,
    usdcMint,
    subscriberTokenAccount,
    amount * maxPulls + amount,
  );

  const mandate = deriveMandateAddress(subscriber.publicKey, planAddresses.plan, context.programId);
  const credentialAta = getAssociatedTokenAddressSync(
    planAddresses.credentialMint,
    subscriber.publicKey,
    false,
    TOKEN_2022_PROGRAM_ID,
  );

  context.svm.expireBlockhash();
  await (context.program as any).methods
    .subscribe()
    .accounts({
      subscriber: subscriber.publicKey,
      merchant: context.merchant.publicKey,
      plan: planAddresses.plan,
      mandate,
      subscriberTokenAccount,
      usdcMint,
      credentialMint: planAddresses.credentialMint,
      subscriberCredentialAccount: credentialAta,
      tokenProgram: TOKEN_PROGRAM_ID,
      token2022Program: TOKEN_2022_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      rent: SYSVAR_RENT_PUBKEY,
    })
    .signers([subscriber])
    .rpc();

  return {
    ...context,
    subscriber,
    planAddresses,
    mandate,
    credentialAta,
    usdcMint,
    subscriberTokenAccount,
    merchantTokenAccount,
    amount,
    frequency,
    maxPulls,
  };
}

export { ACCOUNT_SIZE, DECIMALS, PROGRAM_ID, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID };
