// The README's TypeScript program, `examples/support-agent`, run as a reader runs it: from an empty directory,
// against a server whose `openai` gateway deployment forwards the judge to a local model, with the program's own
// OpenAI call pointed at the same local model.
import { execFile } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

import { expect, vi } from "vitest";
import { WyrdClient } from "wyrd-ai";

import { judgeReply } from "../support/assistant.js";
import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { configureGateway } from "../support/local-development.js";
import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 300_000 });

const EXAMPLE = resolve(import.meta.dirname, "../../../../../examples/support-agent");

/** The support agent's system prompt, which the local model answers with {@link ANSWER}. */
const AGENT = "You are a support agent.";

/** The local model's reply as the support agent. */
const ANSWER = "Refunds are possible within 30 days of purchase, so this order no longer qualifies.";

/** The judge's prompt: it fails an answer promising a guaranteed refund and passes every other. */
const JUDGE = "Does this support answer follow the policy?";

/** The local model: the support agent for {@link AGENT}, the judge for {@link JUDGE}. */
function model({ body }: ReceivedRequest): Reply {
  if (body.includes(AGENT)) {
    return judgeReply(ANSWER);
  }
  if (!body.includes(JUDGE)) {
    return { status: 404 };
  }
  const broken = body.includes("guaranteed refund");
  return judgeReply(
    JSON.stringify({
      outcome: broken ? "failed" : "passed",
      reason: broken
        ? "the answer promises a refund the policy does not allow"
        : "the answer follows the 30-day policy",
      citations: [{ slot: "answer", locator: { json_pointer: "" } }],
    }),
  );
}

const test = serverTest({ provider: model, verificationRuntime: true }).extend<{ reader: string }>({
  reader: async ({ provider }, use) => {
    await configureGateway(WyrdClient.connect());
    vi.stubEnv("OPENAI_BASE_URL", `${provider.url}/v1`);
    vi.stubEnv("OPENAI_API_KEY", "sk-local-upstream");
    await use(mkdtempSync(join(tmpdir(), "wyrd-ts-support-reader-")));
  },
});

/** Run the program in `reader` and return what it printed; the model keeps answering meanwhile. */
async function run(reader: string): Promise<string> {
  const { stdout } = await promisify(execFile)(process.execPath, [join(EXAMPLE, "support_agent.ts")], {
    cwd: reader,
    encoding: "utf8",
  });
  return stdout;
}

test("verifies the support agent's reply from a reader's directory", async ({ reader, provider }) => {
  const output = readFileSync(join(EXAMPLE, "output.txt"), "utf8");
  expect(await run(reader)).toBe(output);

  const calls = provider.received.filter(({ path }) => path === "/v1/chat/completions").map(({ body }) => body);
  expect(
    calls.filter((body) => body.includes(AGENT)),
    "the agent answers once",
  ).toHaveLength(1);
  const judged = calls.filter((body) => body.includes(JUDGE));
  expect(judged, "the judge reads each reply").toHaveLength(2);
  expect(
    judged.every((body) => body.includes("45 days ago")),
    "the judge sees the question",
  ).toBe(true);

  expect(await run(reader), "a rerun declares nothing new and judges the same").toBe(output);
});
