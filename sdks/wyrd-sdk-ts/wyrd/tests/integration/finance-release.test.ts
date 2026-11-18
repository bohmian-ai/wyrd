// The docs Quickstart's TypeScript program, `examples/finance-release`, run
// as a reader runs it: from a directory holding only the downloaded policy PDF, against a server whose `openai`
// gateway deployment forwards the policy judge to a local model that reads the policy extraction first.
import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import { copyFileSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { promisify } from "node:util";

import { expect, vi } from "vitest";
import { Cards, verify, WyrdClient, WyrdState } from "wyrd-ai";

import { sharingPolicyJudge } from "../support/assistant.js";
import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { configureGateway } from "../support/local-development.js";
import { details, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 300_000 });

const EXAMPLE = resolve(import.meta.dirname, "../../../../../examples/finance-release");

/** The example judge's prompt, which the local model answers as the sharing-policy judge. */
const JUDGE = "Decide whether the company sharing policy permits sharing this report";

/** A permitted release's evidence. */
const PERMITTED = {
  purpose: "external audit",
  report: { text: "Synthetic customer report: quarterly total $120." },
  recipient: { email: "auditor@example.test", authorized: true },
  destination: { url: "https://auditor.example.test/reports", restricted: true },
};

/** What the program prints under the original policy, as the docs show it. */
const FIRST_RUN = readFileSync(join(EXAMPLE, "output.txt"), "utf8");

const test = serverTest({
  provider: (request: ReceivedRequest): Reply =>
    request.body.includes(JUDGE) ? sharingPolicyJudge(request, true) : { status: 404 },
  verificationRuntime: true,
}).extend<{ reader: string }>({
  reader: [
    async ({ server: _ }, use) => {
      await configureGateway(WyrdClient.connect());
      const reader = mkdtempSync(join(tmpdir(), "wyrd-ts-finance-reader-"));
      copyFileSync(join(EXAMPLE, "company-sharing-policy.pdf"), join(reader, "company-sharing-policy.pdf"));
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

/** A requirement with the policy file its check mounts for the judge. */
type PolicyRequirement = {
  readonly id: string;
  readonly check: { readonly workspace: Record<string, { readonly artifact: { readonly card: { version: string } } }> };
};

/** The policy Artifact file the `follows-policy` requirement of a Verifier spec or execution outcome mounts for its judge. */
function policyFile(owner: object): unknown {
  const { requirements } = owner as { readonly requirements: readonly PolicyRequirement[] };
  const followsPolicy = requirements.find(({ id }) => id === "follows-policy");
  return followsPolicy?.check.workspace["/inputs/company-sharing-policy.pdf.json"]?.artifact;
}

test("runs the finance release Quickstart from a reader's directory", async ({ reader, provider }) => {
  const first = await run(reader);
  expect(first).toBe(FIRST_RUN);
  expect(await run(reader), "the rerun declares nothing new").toBe(first);
  const verdicts = verdictRequests(provider);
  expect(verdicts).toHaveLength(8);
  expect(
    verdicts.every(
      (body) => body.includes("external-audit") && body.includes("/inputs/company-sharing-policy.pdf.json"),
    ),
    "each verdict follows a read of the policy extraction",
  ).toBe(true);
  expect(
    verdicts.some((body) => body.includes("marketing partnership")),
    "the judge sees each release's purpose",
  ).toBe(true);

  // An execution under the original policy, kept for inspection after the update.
  process.chdir(reader);
  const { service } = await import(join(EXAMPLE, "finance_service.ts"));
  const state = await WyrdState.declare(service);
  const earlier = await state.run().observe.verify("release-check", PERMITTED);
  await state.shutdown();

  // The reader downloads the revised policy over the original and reruns.
  copyFileSync(join(EXAMPLE, "company-sharing-policy-revised.pdf"), join(reader, "company-sharing-policy.pdf"));
  expect(await run(reader), "the revised policy releases the regulator request").toBe(
    FIRST_RUN.replace(
      "regulator request · Synthetic customer report: quarterly total $120. → failed\n" +
        "  follows-policy: the policy does not permit sharing for this purpose\n",
      "regulator request · Synthetic customer report: quarterly total $120. → passed\n",
    ),
  );

  const cards = await Cards.connect();
  const originalCheck = await cards.get({ kind: "Verifier", name: "release-check", version: "0.1.0" });
  const revisedCheck = await cards.get({ kind: "Verifier", name: "release-check", version: "0.1.1" });
  const revisedService = await cards.get({ kind: "Service", name: "finance-release", version: "0.1.1" });
  expect(policyFile(originalCheck.spec)).toMatchObject({ card: { version: "0.1.0" } });
  expect(policyFile(revisedCheck.spec)).toMatchObject({ card: { version: "0.1.1" } });
  expect(revisedService.spec).toMatchObject({ verified_by: [{ verifier: { version: "0.1.1" } }] });

  // The earlier execution still names the original policy file.
  const earlierDetails = await details(earlier.execution_id);
  expect(earlierDetails.verifier.version).toBe("0.1.0");
  expect(policyFile(earlierDetails.outcome)).toEqual(policyFile(originalCheck.spec));
});

test("judges each finance requirement on its own", async ({ reader }) => {
  // The declared Service, loaded from the reader's directory, which holds its policy.
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
    verdicts.push((await run.observe.verify("release-check", release)).outcome.verdict);
  }
  await state.shutdown();
  expect(verdicts).toEqual(["failed", "failed", "inconclusive"]);
});

/** The finance API started in `reader` beside its pulled `.wyrd-bundle`, acting as `key`, and the address it printed. */
async function serveApi(reader: string, key: string) {
  const api = spawn(process.execPath, [join(EXAMPLE, "api.ts")], {
    cwd: reader,
    env: { ...process.env, PORT: "0", WYRD_API_KEY: key },
    stdio: ["ignore", "pipe", "inherit"],
  });
  const [line] = await once(createInterface({ input: api.stdout }), "line");
  return { api, url: String(line).replace("listening on ", "") };
}

/** A finance API response: its decision and, when a verdict decided it, the Run and execution behind it. */
type Decision = { readonly decision: string; readonly run_id?: string; readonly execution_id?: string };

/** `response` with its run and execution IDs replaced by one placeholder, so a decision compares with the guide's. */
function decision(response: Decision) {
  return { ...response, ...("run_id" in response ? { run_id: "<id>", execution_id: "<id>" } : {}) };
}

test("the finance API releases only passed reports", async ({ reader, server }) => {
  process.chdir(reader);
  await run(reader);
  await (await Cards.connect()).pull(
    { kind: "Service", name: "finance-release", version: "0.1.0" },
    join(reader, ".wyrd-bundle"),
  );
  const request = (name: string) => JSON.parse(readFileSync(join(EXAMPLE, name), "utf8"));
  const uncertain = { ...PERMITTED, report: { text: "Synthetic report flagged PROVIDER-UNCERTAIN." } };
  const send = async (url: string, body: unknown) => {
    const response = await fetch(`${url}/reports/send`, { method: "POST", body: JSON.stringify(body) });
    return [response.status, (await response.json()) as Decision] as const;
  };

  const { api, url } = await serveApi(reader, process.env.WYRD_API_KEY ?? "");
  const responses = [];
  for (const body of [request("permitted.json"), request("bank-account.json"), uncertain]) {
    responses.push(await send(url, body));
  }
  api.kill();

  expect(responses.map(([status]) => status)).toEqual([200, 200, 200]);
  const shown = readFileSync(join(EXAMPLE, "api-output.txt"), "utf8")
    .trim()
    .split("\n")
    .map((line) => JSON.parse(line));
  expect(responses.slice(0, 2).map(([, body]) => decision(body))).toEqual(shown.map(decision));
  const [, , [, inconclusive] = [0, { decision: "missing" }]] = responses;
  expect(decision(inconclusive)).toEqual({
    decision: "held for review",
    verdict: "inconclusive",
    run_id: "<id>",
    execution_id: "<id>",
    requirements: { "follows-policy": "inconclusive" },
  });
  // Each decision names the execution whose verdict decided it, so it follows the verdict.
  for (const [, body] of responses) {
    const result = await (await verify.Execution.connect(body.execution_id ?? "")).wait({ timeoutMs: 60_000 });
    expect(result.passed, "only a passed verdict releases").toBe(body.decision === "released");
    expect(result.run_id).toBe(body.run_id);
  }
  expect(new Set(responses.map(([, body]) => body.run_id)).size, "each request runs in a Run of its own").toBe(3);

  // A credential that may not run the Verifier: nothing is judged, so the typed error holds the report.
  const viewer = await serveApi(reader, server.bootstrapService(["viewer"], "finance-api-viewer"));
  const refused = await send(viewer.url, request("permitted.json"));
  viewer.api.kill();
  expect(refused).toEqual([503, { decision: "held for review", error: "WYRD_PERMISSION_403_DENIED_RBAC" }]);
});
