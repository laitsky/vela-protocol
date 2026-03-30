import { describe, expect, test } from "bun:test";
import { getAccount } from "@solana/spl-token";
import { createSubscriptionFixture, TOKEN_2022_PROGRAM_ID } from "./setup";

describe("subscribe", () => {
  test("creates a mandate, delegate approval, and credential token", async () => {
    const fixture = await createSubscriptionFixture();
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

    expect(mandate.subscriber.toBase58()).toBe(fixture.subscriber.publicKey.toBase58());
    expect(mandate.plan.toBase58()).toBe(fixture.planAddresses.plan.toBase58());
    expect(mandate.amount.toString()).toBe(fixture.amount.toString());
    expect(mandate.frequency.toString()).toBe(fixture.frequency.toString());
    expect(mandate.maxPulls.toString()).toBe(fixture.maxPulls.toString());
    expect(Object.keys(mandate.status)[0]).toBe("active");
    expect(subscriberToken.delegate?.toBase58()).toBe(fixture.mandate.toBase58());
    expect(subscriberToken.delegatedAmount).toBe(fixture.amount * fixture.maxPulls);
    expect(credentialAccount.amount).toBe(1n);
  });
});
