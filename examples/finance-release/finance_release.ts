import { type VerificationResult, WyrdState } from "wyrd-ai";

import { type Release, service } from "./finance_service.ts";

// ANCHOR: verify
// Upload the policy, declare every Card, and get a ready state.
const state = await WyrdState.declare(service);

async function verifyRelease(release: Release): Promise<VerificationResult> {
  return state.run().observe.verify("release-check", release);
}
// ANCHOR_END: verify

const recipient = { email: "auditor@example.test", authorized: true };
const destination = { url: "https://auditor.example.test/reports", restricted: true };
for (const [purpose, text] of [
  ["external audit", "Synthetic customer report: quarterly total $120."],
  ["external audit", "Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001."],
  ["marketing partnership", "Synthetic customer report: quarterly total $120."],
  ["regulator request", "Synthetic customer report: quarterly total $120."],
]) {
  const release: Release = { purpose, report: { text }, recipient, destination };
  const result = await verifyRelease(release);
  console.log(`${purpose} · ${text} → ${result.outcome.verdict}`);
  for (const requirement of result.outcome.requirements) {
    if (requirement.outcome !== "passed") {
      console.log(`  ${requirement.id}: ${requirement.reason}`);
    }
  }
}
await state.shutdown();
