import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { context, trace } from "@opentelemetry/api";
import { afterEach, expect, vi } from "vitest";

import { Bifrost, Cards, Gateway, WyrdClient, WyrdState } from "wyrd-ai";

import {
  check,
  deploy,
  explain,
  question,
  REQUESTS,
  serve,
  uid,
  waitForVerdicts,
} from "../../../../../examples/support-desk/typescript/support-desk.js";
import { COMPLETION, configureGateway } from "../support/local-development.js";
import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 300_000 });

/** Upstream answers by the first marker the request body contains. */
const ANSWERS = [
  [
    "list its refund rules",
    '{"findings":[{"rule":"Refunds of up to 100 dollars are approved within 30 days.","page":1}]}',
  ],
  [
    "Claim:",
    '{"outcome":"passed","reason":"consistent with the policy","citations":[{"slot":"policy","locator":{"page":1}}]}',
  ],
  ["refund", "You have a guaranteed refund."],
  ["Answer the customer", "Your order is on its way."],
] as const;

/** A verification-capable deployment that answers every Gateway call by body marker. */
const test = serverTest({
  verificationRuntime: true,
  provider: ({ body }) => {
    const content = ANSWERS.find(([marker]) => body.includes(marker))?.[1] ?? "";
    const [choice] = COMPLETION.choices;
    return { status: 200, body: { ...COMPLETION, choices: [{ ...choice, message: { ...choice?.message, content } }] } };
  },
});

afterEach(() => {
  trace.disable();
  context.disable();
});

test("support desk answers, verifies, and explains every request", async ({ server, provider }) => {
  const admin = WyrdClient.connect({
    serverUrl: server.baseUrl,
    credential: server.tenantAdminKey(),
    grpcUrl: server.grpcUrl,
  });
  const scratch = mkdtempSync(join(tmpdir(), "wyrd-ts-desk-"));
  await (await Gateway.connect({ client: admin })).putDeployment({
    name: "anthropic",
    adapter: "anthropic",
    auth: "none",
  });
  await expect(deploy(admin, join(scratch, "refused"))).rejects.toThrow(
    "configure a gateway provider deployment for openai/gpt-4o",
  );
  await configureGateway(admin);
  const bundle = join(scratch, "bundle");

  const desk = await deploy(admin, bundle);

  const tickets = await (await Bifrost.connect({ client: admin })).describeTable("vala.datasets", "tickets");
  expect(tickets.user_fields.map((field) => field.name)).toEqual(["ticket_id", "question", "answer", "refund"]);
  await expect(
    (await Cards.connect({ client: admin })).registerFromPath(fixture("cards/support_desk/conflicting-desk.yaml")),
  ).rejects.toMatchObject({ code: "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH" });
  const viewer = WyrdClient.connect({
    serverUrl: server.baseUrl,
    credential: server.bootstrapService(["viewer"], "desk-viewer"),
    grpcUrl: server.grpcUrl,
  });
  await expect(
    WyrdState.fromPath(bundle, { client: viewer }).run("agent").invoke({ question: "Where is order 1?" }),
  ).rejects.toMatchObject({ code: "WYRD_PERMISSION_403_DENIED_RBAC" });
  expect(provider.received, "the refusal precedes upstream IO").toEqual([]);

  const promise = await check(desk, "You have a guaranteed refund.");
  expect(promise.outcome.verdict).toBe("failed");
  expect(promise.outcome.requirements.map((requirement) => [requirement.id, requirement.outcome])).toEqual([
    ["no-refund-promise", "failed"],
    ["follows-policy", "passed"],
  ]);
  expect(promise.outcome.requirements.find(({ id }) => id === "follows-policy")?.citations).toEqual([
    { slot: "policy", locator: { page: 1 } },
  ]);

  const served = await serve(desk);
  expect(served).toHaveLength(REQUESTS);
  const results = await waitForVerdicts(admin, served, 180_000);
  expect(results.map((result) => [result.outcome.verdict, result.run_id])).toEqual(
    served.map((request, index) => [question(index).includes("refund") ? "failed" : "passed", request.runId]),
  );

  const agent = uid(desk, "support-agent");
  for (const [index, verdict] of [
    [1, "passed"],
    [0, "failed"],
  ] as const) {
    const runId = served[index]?.runId ?? "";
    expect(await explain(admin, runId)).toMatchObject({
      run_id: runId,
      ticket_id: `T-${index}`,
      span: "support-desk.request",
      call_card_uid: agent,
      verdict,
    });
  }
});
