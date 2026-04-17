import { describe, expect, test } from "bun:test";
import { BN } from "@coral-xyz/anchor";
import { getAccount } from "@solana/spl-token";
import { Keypair, LAMPORTS_PER_SOL, SystemProgram } from "@solana/web3.js";
import {
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  advanceClock,
  airdropSol,
  createToken2022Ata,
  createSubscriptionFixture,
  derivePullApprovalAddress,
  installPhase7AdminState,
  insertPullApproval,
  deriveMintAuthorityAddress,
  TRANSFER_HOOK_PROGRAM_ID,
} from "./setup";

describe("execute_pull", () => {
  test("executes the recurring transfer through anchor-litesvm", async () => {
    const fixture = await createSubscriptionFixture();
    const adminState = await installPhase7AdminState({
      provider: fixture.provider,
      svm: fixture.svm,
      admin: fixture.merchant,
      splUsdcMint: fixture.usdcMint,
    });
    const [mintAuthority] = deriveMintAuthorityAddress();
    const [pullApproval] = derivePullApprovalAddress(fixture.mandate);
    const subscriberWrappedAccount = await createToken2022Ata(
      fixture.provider,
      fixture.mandate,
      adminState.wrappedUsdcMint,
    );
    const merchantWrappedAccount = await createToken2022Ata(
      fixture.provider,
      fixture.merchant.publicKey,
      adminState.wrappedUsdcMint,
    );

    fixture.svm.expireBlockhash();
    await (fixture.program as any).methods
      .wrap(new BN((fixture.amount * fixture.maxPulls).toString()))
      .accounts({
        subscriber: fixture.subscriber.publicKey,
        config: adminState.config,
        splUsdcMint: fixture.usdcMint,
        wrappedUsdcMint: adminState.wrappedUsdcMint,
        subscriberUsdcAccount: fixture.subscriberTokenAccount,
        destinationWrappedAccount: subscriberWrappedAccount,
        destinationAuthority: fixture.mandate,
        wrappingVault: adminState.wrappingVault,
        mintAuthority,
        splTokenProgram: TOKEN_PROGRAM_ID,
        token2022Program: TOKEN_2022_PROGRAM_ID,
      })
      .signers([fixture.subscriber])
      .rpc();

    airdropSol(
      fixture.svm,
      fixture.merchant.publicKey,
      2n * BigInt(LAMPORTS_PER_SOL),
    );

    const mandateBefore = await (fixture.program.account as any).velaMandate.fetch(
      fixture.mandate,
    );
    advanceClock(fixture.svm, BigInt(mandateBefore.nextPaymentDue.toString()));
    insertPullApproval({
      svm: fixture.svm,
      mandate: fixture.mandate,
      validUntil: BigInt(mandateBefore.nextPaymentDue.toString()),
      approvedAmount: fixture.amount,
    });

    fixture.svm.expireBlockhash();
    await (fixture.program as any).methods
      .executePull()
      .accounts({
        payer: fixture.merchant.publicKey,
        subscriber: fixture.subscriber.publicKey,
        merchant: fixture.merchant.publicKey,
        keeperConfig: adminState.keeperConfig,
        plan: fixture.planAddresses.plan,
        mandate: fixture.mandate,
        subscriberWrappedAccount,
        merchantWrappedAccount,
        wrappedUsdcMint: adminState.wrappedUsdcMint,
        pullApproval,
        tokenConfig: adminState.tokenConfig,
        protocolConfig: adminState.config,
        wrappingVault: adminState.wrappingVault,
        hookProgram: TRANSFER_HOOK_PROGRAM_ID,
        extraAccountMetaList: adminState.extraAccountMetaList,
        protocolProgram: fixture.programId,
        token2022Program: TOKEN_2022_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const mandateAfter = await (fixture.program.account as any).velaMandate.fetch(
      fixture.mandate,
    );
    const subscriberToken = await getAccount(
      fixture.provider.connection,
      subscriberWrappedAccount,
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );
    const merchantToken = await getAccount(
      fixture.provider.connection,
      merchantWrappedAccount,
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    expect(mandateAfter.pullsExecuted.toNumber()).toBe(1);
    expect(merchantToken.amount).toBe(fixture.amount);
    expect(subscriberToken.amount).toBe(
      fixture.amount * fixture.maxPulls - fixture.amount,
    );
  });
});
