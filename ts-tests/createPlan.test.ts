import { describe, expect, test } from "bun:test";
import { getMint } from "@solana/spl-token";
import {
  createPlan,
  createUsdcMint,
  deriveMerchantCredentialMint,
  derivePlanAddresses,
  fetchMerchantState,
  installPhase7AdminState,
  initMerchantCredential,
  setupTest,
  TOKEN_2022_PROGRAM_ID,
} from "./setup";

describe("create_plan", () => {
  test("creates a plan PDA and credential mint", async () => {
    const context = setupTest();
    const usdcMint = await createUsdcMint(context.provider);
    const adminState = await installPhase7AdminState({
      provider: context.provider,
      svm: context.svm,
      admin: context.merchant,
      splUsdcMint: usdcMint,
    });
    await initMerchantCredential(context);
    const addresses = await createPlan(
      context,
      25_000_000n,
      3_600n,
      0n,
      4n,
      0n,
      adminState.wrappedUsdcMint,
    );

    const merchantState = await fetchMerchantState(context, addresses.merchantState);
    const plan = await (context.program.account as any).velaPlan.fetch(addresses.plan);
    const [merchantCredentialMint] = deriveMerchantCredentialMint(
      context.merchant.publicKey,
      context.programId,
    );
    const credentialMint = await getMint(
      context.provider.connection,
      merchantCredentialMint,
      undefined,
      TOKEN_2022_PROGRAM_ID,
    );
    const derived = derivePlanAddresses(context.merchant.publicKey, 0n, context.programId);

    expect(addresses.plan.toBase58()).toBe(derived.plan.toBase58());
    expect(merchantState.planCount).toBe(1n);
    expect(plan.planId.toNumber()).toBe(0);
    expect(plan.amount.toString()).toBe("25000000");
    expect(plan.frequency.toString()).toBe("3600");
    expect(plan.maxPulls.toString()).toBe("4");
    expect(plan.credentialMint.toBase58()).toBe(merchantCredentialMint.toBase58());
    expect(credentialMint.decimals).toBe(0);
  });
});
