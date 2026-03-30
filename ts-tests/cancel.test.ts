import { describe, expect, test } from "bun:test";
import { getAccount } from "@solana/spl-token";
import { createSubscriptionFixture, TOKEN_2022_PROGRAM_ID, TOKEN_PROGRAM_ID } from "./setup";

describe("cancel", () => {
  test("burns the credential and revokes delegation", async () => {
    const fixture = await createSubscriptionFixture();

    fixture.svm.expireBlockhash();
    await (fixture.program as any).methods
      .cancel()
      .accounts({
        authority: fixture.subscriber.publicKey,
        subscriber: fixture.subscriber.publicKey,
        plan: fixture.planAddresses.plan,
        mandate: fixture.mandate,
        subscriberCredentialAccount: fixture.credentialAta,
        credentialMint: fixture.planAddresses.credentialMint,
        token2022Program: TOKEN_2022_PROGRAM_ID,
        subscriberTokenAccount: fixture.subscriberTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([fixture.subscriber])
      .rpc();

    const mandate = await (fixture.program.account as any).velaMandate.fetch(
      fixture.mandate,
    );
    const subscriberToken = await getAccount(
      fixture.provider.connection,
      fixture.subscriberTokenAccount,
    );
    const credentialAccount = await getAccount(
      fixture.provider.connection,
      fixture.credentialAta,
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );

    expect(Object.keys(mandate.status)[0]).toBe("cancelled");
    expect(subscriberToken.delegate).toBeNull();
    expect(credentialAccount.amount).toBe(0n);
  });
});
