import { describe, expect, test } from "bun:test";
import { getAccount } from "@solana/spl-token";
import { Keypair, LAMPORTS_PER_SOL } from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  advanceClock,
  airdropSol,
  createSubscriptionFixture,
} from "./setup";

describe("execute_pull", () => {
  test("executes the recurring transfer through anchor-litesvm", async () => {
    const fixture = await createSubscriptionFixture();
    const thirdParty = Keypair.generate();
    airdropSol(fixture.svm, thirdParty.publicKey, 2n * BigInt(LAMPORTS_PER_SOL));

    const mandateBefore = await (fixture.program.account as any).velaMandate.fetch(
      fixture.mandate,
    );
    advanceClock(fixture.svm, BigInt(mandateBefore.nextPaymentDue.toString()));

    fixture.svm.expireBlockhash();
    await (fixture.program as any).methods
      .executePull()
      .accounts({
        payer: thirdParty.publicKey,
        subscriber: fixture.subscriber.publicKey,
        merchant: fixture.merchant.publicKey,
        plan: fixture.planAddresses.plan,
        mandate: fixture.mandate,
        subscriberTokenAccount: fixture.subscriberTokenAccount,
        merchantTokenAccount: fixture.merchantTokenAccount,
        usdcMint: fixture.usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([thirdParty])
      .rpc();

    const mandateAfter = await (fixture.program.account as any).velaMandate.fetch(
      fixture.mandate,
    );
    const subscriberToken = await getAccount(
      fixture.provider.connection,
      fixture.subscriberTokenAccount,
    );
    const merchantToken = await getAccount(
      fixture.provider.connection,
      fixture.merchantTokenAccount,
    );

    expect(mandateAfter.pullsExecuted.toNumber()).toBe(1);
    expect(merchantToken.amount).toBe(fixture.amount);
    expect(subscriberToken.amount).toBe(fixture.amount * fixture.maxPulls);
  });
});
