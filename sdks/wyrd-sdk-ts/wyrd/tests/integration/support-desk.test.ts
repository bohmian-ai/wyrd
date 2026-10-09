import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { context, trace } from "@opentelemetry/api";
import { afterEach, expect, vi } from "vitest";

import { Bifrost, Cards, WyrdClient, WyrdState } from "@wyrd/sdk";

import {
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
  ["Grade the support answer", '{"passed":true}'],
  ["refund", "You have a guaranteed refund."],
  ["Answer the customer", "Your order is on its way."],
] as const;

/** A deployment that verifies continuously and answers every Gateway call by body marker. */
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
  await configureGateway(admin);
  const bundle = join(mkdtempSync(join(tmpdir(), "wyrd-ts-desk-")), "bundle");

  const desk = await deploy(admin, bundle);

  const tickets = await (await Bifrost.connect({ client: admin })).describeTable("vala.datasets", "tickets");
  expect(tickets.user_fields.map((field) => field.name)).toEqual(["ticket_id", "question", "answer", "refund"]);
  await expect(
    Cards.connect({ client: admin }).registerFromPath(fixture("cards/support_desk/conflicting-desk.yaml")),
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

  const served = await serve(desk);
  expect(served.map((request) => request.passed)).toEqual(
    Array.from({ length: REQUESTS }, (_, index) => !question(index).includes("refund")),
  );
  expect(await waitForVerdicts(admin, desk, 180_000)).toEqual({
    "answer-quality": [100, 0],
    "no-refund-promise": [90, 10],
  });

  const agent = uid(desk, "support-agent");
  for (const [index, realtime] of [
    [1, "passed"],
    [0, "failed"],
  ] as const) {
    const runId = served[index]?.runId ?? "";
    expect(await explain(admin, runId)).toMatchObject({
      run_id: runId,
      ticket_id: `T-${index}`,
      span: "support-desk.request",
      call_card_uid: agent,
      continuous: "passed",
      realtime,
    });
  }
});
