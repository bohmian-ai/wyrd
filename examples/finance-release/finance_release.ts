import { WyrdState } from "wyrd-ai";

import { type Release, service } from "./finance_service.ts";

// Upload the policy, declare every Card, and get a ready state.
const state = await WyrdState.declare(service);
const run = state.run();

const recipient = { email: "auditor@example.test", authorized: true };
const destination = { url: "https://auditor.example.test/reports", restricted: true };
for (const text of [
  "Synthetic customer report: quarterly total $120.",
  "Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.",
]) {
  const release: Release = { report: { text }, recipient, destination };
  const result = await run.observe.verify("release-check", release);
  console.log(`${result.outcome.verdict}: ${text}`);
}

await state.shutdown();
