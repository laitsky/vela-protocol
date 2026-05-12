import { createHash } from "node:crypto";
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
  TransactionInstruction,
} from "@solana/web3.js";
import { LiteSVM } from "anchor-litesvm/node_modules/litesvm";
import { existsSync } from "node:fs";
import { join } from "node:path";
import idl from "../target/idl/vela_protocol.json";
import transferHookIdl from "../target/idl/vela_transfer_hook.json";

const PROGRAM_ID = new PublicKey(idl.address);
const PROGRAM_SO_PATH = join(
  process.cwd(),
  "target",
  "deploy",
  "vela_protocol.so",
);
const TRANSFER_HOOK_PROGRAM_ID = new PublicKey(transferHookIdl.address);
const TRANSFER_HOOK_SO_PATH = join(
  process.cwd(),
  "target",
  "deploy",
  "vela_transfer_hook.so",
);
const DECIMALS = 6;
const CONFIG_SEED = Buffer.from("config");
const KEEPER_CONFIG_SEED = Buffer.from("keeper-config");
const MINT_AUTHORITY_SEED = Buffer.from("mint-authority");
const EXTRA_ACCOUNT_METAS_SEED = Buffer.from("extra-account-metas");
const TOKEN_CONFIG_SEED = Buffer.from("token_config");
const APPROVAL_SEED = Buffer.from("approval");
const EMPTY_PUBKEY = new PublicKey(new Uint8Array(32));
const PROTOCOL_CONFIG_SIZE = 220;
const KEEPER_CONFIG_SIZE = 268;
const PULL_APPROVAL_SIZE = 82;
const TOKEN_CONFIG_SIZE = 213;

type VelaProgram = Program<any>;

type PlanAddresses = {
  merchantState: PublicKey;
  plan: PublicKey;
  credentialMint: PublicKey;
};

type TestContext = {
  svm: LiteSVM;
  provider: LiteSVMProvider;
  program: VelaProgram;
  programId: PublicKey;
  merchant: Keypair;
};

type SubscriptionFixture = TestContext & {
  subscriber: Keypair;
  planAddresses: PlanAddresses;
  mandate: PublicKey;
  credentialAta: PublicKey;
  usdcMint: PublicKey;
  config: PublicKey;
  keeperConfig: PublicKey;
  wrappedUsdcMint: PublicKey;
  wrappingVault: PublicKey;
  extraAccountMetaList: PublicKey;
  tokenConfig: PublicKey;
  subscriberTokenAccount: PublicKey;
  merchantTokenAccount: PublicKey;
  amount: bigint;
  frequency: bigint;
  maxPulls: bigint;
};

function discriminator(namespace: "global" | "account", name: string): Buffer {
  return createHash("sha256")
    .update(`${namespace}:${name}`)
    .digest()
    .subarray(0, 8);
}

function serializeProtocolConfig(
  admin: PublicKey,
  bump: number,
  args: {
    wrappedUsdcMint?: PublicKey;
    wrappingVault?: PublicKey;
    transferHookProgramId?: PublicKey;
  } = {},
): Uint8Array {
  const data = Buffer.alloc(PROTOCOL_CONFIG_SIZE);
  discriminator("account", "ProtocolConfig").copy(data, 0);
  admin.toBuffer().copy(data, 8);
  EMPTY_PUBKEY.toBuffer().copy(data, 40);
  data.writeUInt8(0, 72);
  data.writeBigUInt64LE(0n, 73);
  (args.wrappedUsdcMint ?? EMPTY_PUBKEY).toBuffer().copy(data, 81);
  (args.wrappingVault ?? EMPTY_PUBKEY).toBuffer().copy(data, 113);
  data.writeUInt8(0, 145);
  data.writeBigInt64LE(0n, 146);
  (args.transferHookProgramId ?? EMPTY_PUBKEY).toBuffer().copy(data, 154);
  data.writeUInt8(bump, 186);
  data.writeUInt8(1, 187);
  EMPTY_PUBKEY.toBuffer().copy(data, 188);
  return data;
}

function serializeKeeperConfig(
  admin: PublicKey,
  keeperAuthority: PublicKey,
  bump: number,
): Uint8Array {
  const data = Buffer.alloc(KEEPER_CONFIG_SIZE);
  discriminator("account", "KeeperConfig").copy(data, 0);
  admin.toBuffer().copy(data, 8);
  data.writeUInt8(0, 40);
  data.writeUInt8(0, 169);
  keeperAuthority.toBuffer().copy(data, 170);
  data.writeUInt8(bump, 202);
  data.writeUInt8(1, 203);
  return data;
}

function serializeTokenConfig(args: {
  mint: PublicKey;
  tokenProgram: PublicKey;
  decimals: number;
  admin: PublicKey;
  bump: number;
}): Uint8Array {
  const data = Buffer.alloc(TOKEN_CONFIG_SIZE);
  discriminator("account", "TokenConfig").copy(data, 0);
  args.mint.toBuffer().copy(data, 8);
  args.tokenProgram.toBuffer().copy(data, 40);
  data.writeUInt8(0, 72);
  data.writeUInt8(args.decimals, 73);
  data.writeUInt8(1, 74);
  EMPTY_PUBKEY.toBuffer().copy(data, 75);
  args.admin.toBuffer().copy(data, 107);
  data.writeBigInt64LE(0n, 139);
  data.writeUInt8(args.bump, 147);
  data.writeUInt8(1, 148);
  return data;
}

function serializePullApproval(args: {
  mandate: PublicKey;
  periodStart: bigint;
  periodEnd: bigint;
  validUntil: bigint;
  approved: boolean;
  approvedAmount: bigint;
  createdAt: bigint;
  bump: number;
}): Uint8Array {
  const data = Buffer.alloc(PULL_APPROVAL_SIZE);
  discriminator("account", "PullApproval").copy(data, 0);
  args.mandate.toBuffer().copy(data, 8);
  data.writeBigInt64LE(args.periodStart, 40);
  data.writeBigInt64LE(args.periodEnd, 48);
  data.writeBigInt64LE(args.validUntil, 56);
  data.writeUInt8(args.approved ? 1 : 0, 64);
  data.writeBigUInt64LE(args.approvedAmount, 65);
  data.writeBigInt64LE(args.createdAt, 73);
  data.writeUInt8(args.bump, 81);
  return data;
}

function buildInitWrappedMintInstruction(args: {
  admin: PublicKey;
  config: PublicKey;
  mintAuthority: PublicKey;
  wrappedUsdcMint: PublicKey;
  splUsdcMint: PublicKey;
  wrappingVault: PublicKey;
}): TransactionInstruction {
  return new TransactionInstruction({
    programId: PROGRAM_ID,
    keys: [
      { pubkey: args.admin, isSigner: true, isWritable: true },
      { pubkey: args.config, isSigner: false, isWritable: true },
      { pubkey: args.mintAuthority, isSigner: false, isWritable: false },
      { pubkey: args.wrappedUsdcMint, isSigner: true, isWritable: true },
      { pubkey: args.splUsdcMint, isSigner: false, isWritable: false },
      { pubkey: args.wrappingVault, isSigner: false, isWritable: true },
      { pubkey: TOKEN_2022_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      {
        pubkey: ASSOCIATED_TOKEN_PROGRAM_ID,
        isSigner: false,
        isWritable: false,
      },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
    ],
    data: discriminator("global", "init_wrapped_mint"),
  });
}

function buildInitExtraAccountMetaListInstruction(args: {
  admin: PublicKey;
  config: PublicKey;
  extraAccountMetaList: PublicKey;
  wrappedUsdcMint: PublicKey;
  wrappingVault: PublicKey;
}): TransactionInstruction {
  return new TransactionInstruction({
    programId: TRANSFER_HOOK_PROGRAM_ID,
    keys: [
      { pubkey: args.admin, isSigner: true, isWritable: true },
      { pubkey: args.config, isSigner: false, isWritable: false },
      { pubkey: args.extraAccountMetaList, isSigner: false, isWritable: true },
      { pubkey: args.wrappedUsdcMint, isSigner: false, isWritable: false },
      { pubkey: args.wrappingVault, isSigner: false, isWritable: false },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: discriminator("global", "init_extra_account_meta_list"),
  });
}

export function setupTest(): TestContext {
  if (!existsSync(PROGRAM_SO_PATH) || !existsSync(TRANSFER_HOOK_SO_PATH)) {
    throw new Error(
      `Expected compiled programs at ${PROGRAM_SO_PATH} and ${TRANSFER_HOOK_SO_PATH}`,
    );
  }

  const svm = new LiteSVM().withDefaultPrograms().withTransactionHistory(0n);
  svm.addProgramFromFile(PROGRAM_ID, PROGRAM_SO_PATH);
  svm.addProgramFromFile(TRANSFER_HOOK_PROGRAM_ID, TRANSFER_HOOK_SO_PATH);

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

export async function fetchMerchantState(
  context: TestContext,
  address: PublicKey,
) {
  const account = await context.provider.connection.getAccountInfo(address);
  if (!account) {
    throw new Error("merchant state account not found");
  }

  const data = Buffer.from(account.data);
  const expectedDiscriminator = discriminator("account", "MerchantState");
  if (!data.subarray(0, 8).equals(expectedDiscriminator)) {
    throw new Error("invalid merchant state discriminator");
  }

  return {
    merchant: new PublicKey(data.subarray(8, 40)),
    planCount: data.readBigUInt64LE(40),
    bump: data.readUInt8(48),
    credentialMint: new PublicKey(data.subarray(49, 81)),
    mandateCounter: data.readBigUInt64LE(81),
    version: data.readUInt8(89),
  };
}

function deriveMandateAddress(
  subscriber: PublicKey,
  merchant: PublicKey,
  mandateIndex: bigint,
  programId = PROGRAM_ID,
): PublicKey {
  const indexSeed = new BN(mandateIndex.toString()).toArrayLike(
    Buffer,
    "le",
    8,
  );
  return PublicKey.findProgramAddressSync(
    [
      Buffer.from("mandate"),
      subscriber.toBuffer(),
      merchant.toBuffer(),
      indexSeed,
    ],
    programId,
  )[0];
}

function deriveConfigAddress(programId = PROGRAM_ID): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([CONFIG_SEED], programId);
}

function deriveKeeperConfigAddress(
  programId = PROGRAM_ID,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([KEEPER_CONFIG_SEED], programId);
}

export function deriveMintAuthorityAddress(
  programId = PROGRAM_ID,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync([MINT_AUTHORITY_SEED], programId);
}

function deriveExtraAccountMetaListAddress(
  wrappedUsdcMint: PublicKey,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [EXTRA_ACCOUNT_METAS_SEED, wrappedUsdcMint.toBuffer()],
    TRANSFER_HOOK_PROGRAM_ID,
  );
}

function deriveTokenConfigAddress(
  mint: PublicKey,
  programId = PROGRAM_ID,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [TOKEN_CONFIG_SEED, mint.toBuffer()],
    programId,
  );
}

export function derivePullApprovalAddress(
  mandate: PublicKey,
  programId = PROGRAM_ID,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [APPROVAL_SEED, mandate.toBuffer()],
    programId,
  );
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

async function sendInstructions(
  provider: LiteSVMProvider,
  instructions: Parameters<Transaction["add"]>,
  signers: Keypair[] = [],
): Promise<string> {
  provider.client.expireBlockhash();
  const tx = new Transaction().add(...instructions);
  return provider.sendAndConfirm!(tx, signers);
}

export async function installPhase7AdminState(args: {
  provider: LiteSVMProvider;
  svm: LiteSVM;
  admin: Keypair;
  splUsdcMint: PublicKey;
}): Promise<{
  config: PublicKey;
  keeperConfig: PublicKey;
  mintAuthority: PublicKey;
  wrappedUsdcMint: PublicKey;
  wrappingVault: PublicKey;
  extraAccountMetaList: PublicKey;
  tokenConfig: PublicKey;
}> {
  const { provider, svm, admin, splUsdcMint } = args;
  const [config, configBump] = deriveConfigAddress();
  const [keeperConfig, keeperConfigBump] = deriveKeeperConfigAddress();
  const [mintAuthority] = deriveMintAuthorityAddress();
  const wrappingVault = getAssociatedTokenAddressSync(
    splUsdcMint,
    mintAuthority,
    true,
    TOKEN_PROGRAM_ID,
  );

  svm.setAccount(config, {
    lamports: Number(
      svm.minimumBalanceForRentExemption(BigInt(PROTOCOL_CONFIG_SIZE)),
    ),
    data: serializeProtocolConfig(admin.publicKey, configBump),
    owner: PROGRAM_ID,
    executable: false,
    rentEpoch: 0,
  });

  svm.setAccount(keeperConfig, {
    lamports: Number(
      svm.minimumBalanceForRentExemption(BigInt(KEEPER_CONFIG_SIZE)),
    ),
    data: serializeKeeperConfig(
      admin.publicKey,
      admin.publicKey,
      keeperConfigBump,
    ),
    owner: PROGRAM_ID,
    executable: false,
    rentEpoch: 0,
  });

  const wrappedUsdcMint = Keypair.generate();
  await sendInstructions(
    provider,
    [
      buildInitWrappedMintInstruction({
        admin: admin.publicKey,
        config,
        mintAuthority,
        wrappedUsdcMint: wrappedUsdcMint.publicKey,
        splUsdcMint,
        wrappingVault,
      }),
    ],
    [wrappedUsdcMint],
  );
  svm.setAccount(config, {
    lamports: Number(
      svm.minimumBalanceForRentExemption(BigInt(PROTOCOL_CONFIG_SIZE)),
    ),
    data: serializeProtocolConfig(admin.publicKey, configBump, {
      wrappedUsdcMint: wrappedUsdcMint.publicKey,
      wrappingVault,
      transferHookProgramId: TRANSFER_HOOK_PROGRAM_ID,
    }),
    owner: PROGRAM_ID,
    executable: false,
    rentEpoch: 0,
  });

  const [extraAccountMetaList] = deriveExtraAccountMetaListAddress(
    wrappedUsdcMint.publicKey,
  );
  await sendInstructions(provider, [
    buildInitExtraAccountMetaListInstruction({
      admin: admin.publicKey,
      config,
      extraAccountMetaList,
      wrappedUsdcMint: wrappedUsdcMint.publicKey,
      wrappingVault,
    }),
  ]);
  const [tokenConfig, tokenConfigBump] = deriveTokenConfigAddress(
    wrappedUsdcMint.publicKey,
  );
  svm.setAccount(tokenConfig, {
    lamports: Number(
      svm.minimumBalanceForRentExemption(BigInt(TOKEN_CONFIG_SIZE)),
    ),
    data: serializeTokenConfig({
      mint: wrappedUsdcMint.publicKey,
      tokenProgram: TOKEN_2022_PROGRAM_ID,
      decimals: DECIMALS,
      admin: admin.publicKey,
      bump: tokenConfigBump,
    }),
    owner: PROGRAM_ID,
    executable: false,
    rentEpoch: 0,
  });

  return {
    config,
    keeperConfig,
    mintAuthority,
    wrappedUsdcMint: wrappedUsdcMint.publicKey,
    wrappingVault,
    extraAccountMetaList,
    tokenConfig,
  };
}

export function insertPullApproval(args: {
  svm: LiteSVM;
  mandate: PublicKey;
  periodStart?: bigint;
  periodEnd?: bigint;
  validUntil: bigint;
  approvedAmount: bigint;
  approved?: boolean;
  createdAt?: bigint;
}): PublicKey {
  const { svm, mandate, validUntil, approvedAmount, approved = true } = args;
  const periodEnd = args.periodEnd ?? validUntil;
  const periodStart = args.periodStart ?? periodEnd;
  const createdAt = args.createdAt ?? validUntil;
  const [approval, bump] = derivePullApprovalAddress(mandate);
  svm.setAccount(approval, {
    lamports: Number(
      svm.minimumBalanceForRentExemption(BigInt(PULL_APPROVAL_SIZE)),
    ),
    data: serializePullApproval({
      mandate,
      periodStart,
      periodEnd,
      validUntil,
      approved,
      approvedAmount,
      createdAt,
      bump,
    }),
    owner: PROGRAM_ID,
    executable: false,
    rentEpoch: 0,
  });
  return approval;
}

export async function createToken2022Ata(
  provider: LiteSVMProvider,
  owner: PublicKey,
  mint: PublicKey,
): Promise<PublicKey> {
  const ata = getAssociatedTokenAddressSync(
    mint,
    owner,
    true,
    TOKEN_2022_PROGRAM_ID,
  );
  await sendInstructions(provider, [
    createAssociatedTokenAccountIdempotentInstruction(
      provider.wallet.publicKey,
      ata,
      owner,
      mint,
      TOKEN_2022_PROGRAM_ID,
    ),
  ]);
  return ata;
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

async function createSplTokenAccount(
  provider: LiteSVMProvider,
  owner: PublicKey,
  mint: PublicKey,
): Promise<PublicKey> {
  const ata = getAssociatedTokenAddressSync(
    mint,
    owner,
    false,
    TOKEN_PROGRAM_ID,
  );
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

async function mintUsdc(
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

export function deriveMerchantCredentialMint(
  merchant: PublicKey,
  programId = PROGRAM_ID,
): [PublicKey, number] {
  return PublicKey.findProgramAddressSync(
    [Buffer.from("merchant-credential"), merchant.toBuffer()],
    programId,
  );
}

export async function initMerchantCredential(
  context: TestContext,
): Promise<PublicKey> {
  const [merchantState] = PublicKey.findProgramAddressSync(
    [Buffer.from("merchant"), context.merchant.publicKey.toBuffer()],
    context.programId,
  );
  const [credentialMint] = deriveMerchantCredentialMint(
    context.merchant.publicKey,
    context.programId,
  );
  context.svm.expireBlockhash();
  await (context.program as any).methods
    .initMerchantCredential()
    .accounts({
      merchant: context.merchant.publicKey,
      merchantState,
      credentialMint,
      systemProgram: SystemProgram.programId,
      token2022Program: TOKEN_2022_PROGRAM_ID,
      rent: SYSVAR_RENT_PUBKEY,
    })
    .rpc();
  return credentialMint;
}

export async function createPlan(
  context: TestContext,
  amount: bigint,
  frequency: bigint,
  trialPeriod: bigint,
  maxPulls: bigint,
  planId = 0n,
  billingMint?: PublicKey,
): Promise<PlanAddresses> {
  const addresses = derivePlanAddresses(
    context.merchant.publicKey,
    planId,
    context.programId,
  );
  const resolvedBillingMint =
    billingMint ??
    (
      await (context.program.account as any).protocolConfig.fetch(
        deriveConfigAddress(context.programId)[0],
      )
    ).wrappedUsdcMint;
  const [tokenConfig] = deriveTokenConfigAddress(
    resolvedBillingMint,
    context.programId,
  );
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
      billingMint: resolvedBillingMint,
      tokenConfig,
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

  const usdcMint = await createUsdcMint(context.provider);
  const adminState = await installPhase7AdminState({
    provider: context.provider,
    svm: context.svm,
    admin: context.merchant,
    splUsdcMint: usdcMint,
  });
  await initMerchantCredential(context);

  const planAddresses = await createPlan(
    context,
    amount,
    frequency,
    trialPeriod,
    maxPulls,
    planId,
    adminState.wrappedUsdcMint,
  );
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

  // mandate_counter is 0 for the first subscription in a fresh merchant state
  const mandate = deriveMandateAddress(
    subscriber.publicKey,
    context.merchant.publicKey,
    0n,
    context.programId,
  );
  const [merchantCredentialMint] = deriveMerchantCredentialMint(
    context.merchant.publicKey,
    context.programId,
  );
  const credentialAta = getAssociatedTokenAddressSync(
    merchantCredentialMint,
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
      merchantState: planAddresses.merchantState,
      plan: planAddresses.plan,
      mandate,
      credentialMint: merchantCredentialMint,
      billingMint: adminState.wrappedUsdcMint,
      tokenConfig: adminState.tokenConfig,
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
    config: adminState.config,
    keeperConfig: adminState.keeperConfig,
    wrappedUsdcMint: adminState.wrappedUsdcMint,
    wrappingVault: adminState.wrappingVault,
    extraAccountMetaList: adminState.extraAccountMetaList,
    tokenConfig: adminState.tokenConfig,
    subscriberTokenAccount,
    merchantTokenAccount,
    amount,
    frequency,
    maxPulls,
  };
}

export { TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID, TRANSFER_HOOK_PROGRAM_ID };
