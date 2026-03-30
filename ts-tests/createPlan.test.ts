import { describe, expect, test } from "bun:test";
import { getMint } from "@solana/spl-token";
import { createPlan, derivePlanAddresses, setupTest, TOKEN_2022_PROGRAM_ID } from "./setup";

describe("create_plan", () => {
  test("creates a plan PDA and credential mint", async () => {
    const context = setupTest();
    const addresses = await createPlan(context, 25_000_000n, 3_600n, 0n, 4n);

    const merchantState = await (context.program.account as any).merchantState.fetch(
      addresses.merchantState,
    );
    const plan = await (context.program.account as any).velaPlan.fetch(addresses.plan);
    const credentialMint = await getMint(
      context.provider.connection,
      addresses.credentialMint,
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );
    const derived = derivePlanAddresses(context.merchant.publicKey, 0n, context.programId);

    expect(addresses.plan.toBase58()).toBe(derived.plan.toBase58());
    expect(merchantState.planCount.toNumber()).toBe(1);
    expect(plan.planId.toNumber()).toBe(0);
    expect(plan.amount.toString()).toBe("25000000");
    expect(plan.frequency.toString()).toBe("3600");
    expect(plan.maxPulls.toString()).toBe("4");
    expect(plan.credentialMint.toBase58()).toBe(addresses.credentialMint.toBase58());
    expect(credentialMint.decimals).toBe(0);
    expect(credentialMint.supply).toBe(0n);
  });
});
