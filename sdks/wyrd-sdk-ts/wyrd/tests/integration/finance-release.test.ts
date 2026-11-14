// The docs Quickstart's TypeScript program, `examples/finance-release`, run
// as a reader runs it: from an empty directory, where it saves the policy, against a server whose `openai` gateway
// deployment forwards the policy judge to a local model that reads the policy first.
import { execFile } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

import { expect, vi } from "vitest";
import { WyrdClient, WyrdState } from "wyrd-ai";

import { sharingPolicyJudge } from "../support/assistant.js";
import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { configureGateway } from "../support/local-development.js";
import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 300_000 });

const EXAMPLE = resolve(import.meta.dirname, "../../../../../examples/finance-release");

/** The example judge's prompt, which the local model answers as the sharing-policy judge. */
const JUDGE = "Decide whether the company sharing policy permits sharing this report";

/** A permitted release's evidence. */
const PERMITTED = {
  report: { text: "Synthetic customer report: quarterly total $120." },
  recipient: { email: "auditor@example.test", authorized: true },
  destination: { url: "https://auditor.example.test/reports", restricted: true },
};

const test = serverTest({
  provider: (request: ReceivedRequest): Reply =>
    request.body.includes(JUDGE) ? sharingPolicyJudge(request) : { status: 404 },
  verificationRuntime: true,
}).extend<{ reader: string }>({
  reader: [
    async ({ server: _ }, use) => {
      await configureGateway(WyrdClient.connect());
      const reader = mkdtempSync(join(tmpdir(), "wyrd-ts-finance-reader-"));
      await use(reader);
    },
    { scope: "file" },
  ],
});

/** Run the program in `reader` and return what it printed; the provider keeps answering meanwhile. */
async function run(reader: string): Promise<string> {
  const { stdout } = await promisify(execFile)(process.execPath, [join(EXAMPLE, "finance_release.ts")], {
    cwd: reader,
    encoding: "utf8",
  });
  return stdout;
}

/** The policy judge's verdict requests: each carries the `fs.read` result it judged. */
function verdictRequests(provider: { readonly received: readonly ReceivedRequest[] }): string[] {
  return provider.received
    .map(({ body }) => body)
    .filter((body) => body.includes(JUDGE) && /"role"\s*:\s*"tool"/.test(body));
}

test("runs the finance release Quickstart from a reader's directory", async ({ reader, provider }) => {
  const first = await run(reader);
  expect(first).toBe(
    "passed: Synthetic customer report: quarterly total $120.\n" +
      "failed: Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.\n",
  );
  expect(await run(reader), "the rerun declares nothing new").toBe(first);
  const verdicts = verdictRequests(provider);
  expect(verdicts).toHaveLength(4);
  expect(
    verdicts.every((body) => body.includes("external-audit")),
    "each verdict follows a read of the policy",
  ).toBe(true);
});

test("judges each finance requirement on its own", async ({ reader }) => {
  // The declared Service, loaded from the reader's directory, where it saved its policy.
  process.chdir(reader);
  const { service } = await import(join(EXAMPLE, "finance_service.ts"));
  const state = await WyrdState.declare(service);
  const run = state.run();
  const verdicts = [];
  for (const release of [
    { ...PERMITTED, recipient: { email: "x@example.test", authorized: false } },
    { ...PERMITTED, destination: { url: "https://example.test", restricted: false } },
    { ...PERMITTED, report: { text: "Synthetic report flagged PROVIDER-UNCERTAIN." } },
  ]) {
    verdicts.push((await run.observe.verify("release-check", { release })).outcome.verdict);
  }
  await state.shutdown();
  expect(verdicts).toEqual(["failed", "failed", "inconclusive"]);
});
