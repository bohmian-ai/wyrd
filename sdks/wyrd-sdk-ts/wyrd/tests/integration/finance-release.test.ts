// The docs Quickstart's TypeScript program, `examples/finance-release`, run
// as a reader runs it: from a directory holding only the policy PDF and
// `releases.json`, against a server whose `openai` gateway deployment forwards
// the policy judge to a local model that reads the policy first.
import { execFile } from "node:child_process";
import { cpSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

import { expect, vi } from "vitest";
import { WyrdClient } from "wyrd-ai";

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
      for (const file of ["company-sharing-policy.pdf", "releases.json"]) {
        cpSync(join(EXAMPLE, file), join(reader, file));
      }
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
    "quarterly-report: passed (release-check@0.1.0) -> released\n" +
      "report-with-bank-account: failed (release-check@0.1.0) -> held for review\n" +
      "  follows-policy failed: the report names a bank account identifier\n",
  );
  expect(await run(reader), "the rerun declares nothing new").toBe(first);
  const verdicts = verdictRequests(provider);
  expect(verdicts).toHaveLength(4);
  expect(
    verdicts.every((body) => body.includes("external-audit")),
    "each verdict follows a read of the policy extraction",
  ).toBe(true);
});

test("holds every finance release that does not pass", async ({ reader }) => {
  writeFileSync(
    join(reader, "releases.json"),
    JSON.stringify([
      { name: "unknown-recipient", ...PERMITTED, recipient: { email: "x@example.test", authorized: false } },
      { name: "public-destination", ...PERMITTED, destination: { url: "https://example.test", restricted: false } },
      { name: "uncertain-policy", ...PERMITTED, report: { text: "Synthetic report flagged PROVIDER-UNCERTAIN." } },
      { name: "unreadable-report", ...PERMITTED, report: { text: 123 } },
    ]),
  );
  const lines = (await run(reader)).trimEnd().split("\n");
  expect(lines[0]).toBe("unknown-recipient: failed (release-check@0.1.0) -> held for review");
  expect(lines[1]).toMatch(/^ {2}recipient-authorized failed: /);
  expect(lines[2]).toBe("public-destination: failed (release-check@0.1.0) -> held for review");
  expect(lines[3]).toMatch(/^ {2}destination-restricted failed: /);
  expect(lines.slice(4)).toEqual([
    "uncertain-policy: inconclusive (release-check@0.1.0) -> held for review",
    "  follows-policy inconclusive: the policy does not settle this release",
    "unreadable-report: WYRD_VERIFICATION_400_INPUT_INVALID -> held for review",
  ]);
});
