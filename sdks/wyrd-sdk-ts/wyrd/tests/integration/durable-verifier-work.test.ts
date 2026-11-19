import { createHash } from "node:crypto";

import { expect, vi } from "vitest";

import { type ExecutionInvocation, type PublicationSummary, WyrdClient, verify } from "wyrd-ai";

import { assistantTest } from "../support/assistant.js";
import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 180_000 });

/** How long a test waits for a queued execution to settle. */
const WAIT_MS = 90_000;

/** The stable refusal for a caller no grant authorizes. */
const DENIED = { code: "WYRD_PERMISSION_403_DENIED_RBAC" };

/** The stable refusal of a malformed page or selector. */
const INVALID = { code: "WYRD_SPEC_400_VALIDATION" };

const test = assistantTest({ perTest: true });

/** A server running no verification runtime, so the harness can hold an execution unfinished. */
const held = serverTest({ perTest: true });

/** The manifest entry of `content` retained at `path`. */
function retained(path: string, content: string) {
  const bytes = Buffer.from(content);
  return { path, sha256: createHash("sha256").update(bytes).digest("hex"), size_bytes: bytes.length };
}

/**
 * A settled execution's invocations and publications read back as stable,
 * authorized pages, and cancelling it answers its status every time.
 *
 * Its one requirement attempt reads with its outcome and reason, and pages
 * of one entry end where one page does; a malformed cursor and a limit outside 1–200 are refused, and an
 * attempt that published nothing reads as publication-not-found. A caller
 * with `cards:read` but no `verifier:read` is refused, another tenant sees
 * no such execution, and a viewer reads but cannot cancel. The
 * administrator's cancel and its repeat both answer the settled status.
 */
test("execution_pages_are_stable_and_tenant_scoped", async ({ server, state }) => {
  const submitted = await state.run("agent").observe.submit("answer-check", { answer: "yes" });
  if (submitted.kind !== "queued") {
    throw new Error("Run evidence queues an execution");
  }
  await submitted.execution.wait({ timeoutMs: WAIT_MS });
  const id = submitted.execution.id;
  const execution = await verify.Execution.connect(id);

  const whole = await execution.invocations();
  expect(whole.execution_id).toBe(id);
  expect(whole.status).toBe("completed");
  expect(whole.next_cursor ?? null).toBeNull();
  expect(whole.items).toEqual([
    {
      producer: { step_id: "says-yes", item_index: null, iteration_index: null, attempt: 1 },
      state: "requirement",
      outcome: "passed",
      reason: "the comparison held",
      speculation: "none",
    },
  ]);
  const paged: ExecutionInvocation[] = [];
  let cursor: string | null | undefined;
  do {
    const page = await execution.invocations({ limit: 1, cursor });
    expect(page.items.length).toBeLessThanOrEqual(1);
    paged.push(...page.items);
    cursor = page.next_cursor;
  } while (cursor);
  expect(paged).toEqual(whole.items);

  expect(await execution.publications({ limit: 1 })).toEqual({ execution_id: id, items: [], next_cursor: null });
  for (const limit of [0, 201]) {
    await expect(execution.invocations({ limit })).rejects.toMatchObject(INVALID);
  }
  await expect(execution.publications({ cursor: "not-a-cursor" })).rejects.toMatchObject(INVALID);
  await expect(execution.publication("review", "verdict", { attempt: 0 })).rejects.toMatchObject(INVALID);
  await expect(execution.publication("review", "verdict", { attempt: 2 })).rejects.toMatchObject({
    code: "WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND",
  });
  await expect(execution.download("review", "verdict", { attempt: 1 })).rejects.toMatchObject({
    code: "WYRD_VERIFICATION_404_PUBLICATION_NOT_FOUND",
  });

  const as = (credential: string) => verify.Execution.connect(id, { client: WyrdClient.connect({ credential }) });
  const cardsOnly = await as(server.scopedApiKey("pages_cards", [{ resource: "cards", action: "read", scope: "all" }]));
  await expect(cardsOnly.invocations()).rejects.toMatchObject(DENIED);
  await expect(cardsOnly.publications()).rejects.toMatchObject(DENIED);
  const foreign = await as(
    server.bootstrapServiceInTenant(server.seedTenant("pages-foreign"), ["admin"], "pages_foreign_admin"),
  );
  await expect(foreign.invocations()).rejects.toMatchObject({ code: "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND" });
  await expect(foreign.cancel()).rejects.toMatchObject({ code: "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND" });
  const viewer = await as(server.bootstrapService(["viewer"], "pages_viewer"));
  expect((await viewer.invocations()).items).toEqual(whole.items);
  await expect(viewer.cancel()).rejects.toMatchObject(DENIED);

  for (let repeat = 0; repeat < 2; repeat += 1) {
    expect(await execution.cancel()).toEqual({ execution_id: id, status: "completed" });
  }
});

/**
 * A held execution's publications read back through the public SDK, its
 * files download through their plans, and cancelling it while it runs
 * answers `cancelled` every time.
 *
 * The harness holds the execution with two attempt-1 steps: `review`
 * publishes JSON `{"ok": true}` and JSON `null`, `draft` a file and a
 * directory. Each reads back with its selected producer, the file and
 * directory read back with exact manifests, and each planned download
 * fetches exactly the published bytes from a non-expiring local route. A
 * JSON publication has no download. A grant on exactly this Verifier reads,
 * a viewer cannot cancel, and the administrator's cancel and its repeat both
 * answer `cancelled` with the publications still listed.
 */
held("held_execution_reads_and_downloads_its_publications_and_cancels", async ({ server }) => {
  const { executionId: id, verifier } = server.holdPublications("held-pages");
  const admin = WyrdClient.connect({ credential: server.apiKey });
  const execution = await verify.Execution.connect(id, { client: admin });

  const listed: PublicationSummary[] = [];
  let cursor: string | null | undefined;
  do {
    const page = await execution.publications({ limit: 1, cursor });
    listed.push(...page.items);
    cursor = page.next_cursor;
  } while (cursor);
  expect(listed.map((item) => [item.producer.step_id, item.name, item.kind])).toEqual([
    ["review", "verdict", "json"],
    ["review", "nothing", "json"],
    ["draft", "report", "file"],
    ["draft", "tree", "directory"],
  ]);
  const draft = { step_id: "draft", item_index: null, iteration_index: null, attempt: 1 };
  expect(listed[2]?.producer).toEqual(draft);

  expect(await execution.publication("review", "verdict", { attempt: 1 })).toMatchObject({
    kind: "json",
    value: { ok: true },
    producer: { step_id: "review", attempt: 1 },
  });
  expect(await execution.publication("review", "nothing", { attempt: 1 })).toMatchObject({ kind: "json", value: null });
  expect(await execution.publication("draft", "report", { attempt: 1 })).toEqual({
    name: "report",
    kind: "file",
    producer: draft,
    manifest: { files: [retained("report", "# Report\n")], directories: [] },
  });
  expect(await execution.publication("draft", "tree", { attempt: 1 })).toEqual({
    name: "tree",
    kind: "directory",
    producer: draft,
    manifest: { files: [retained("a.md", "alpha"), retained("nested/b.md", "beta")], directories: ["nested"] },
  });

  for (const [name, path, content] of [
    ["report", null, "# Report\n"],
    ["tree", "nested/b.md", "beta"],
  ] as const) {
    const planned = await execution.download("draft", name, { attempt: 1, path });
    const bytes = Buffer.from(content);
    expect([planned.size_bytes, planned.sha256, planned.plan.ttl_secs]).toEqual([
      bytes.length,
      createHash("sha256").update(bytes).digest("base64"),
      0,
    ]);
    const fetched = await admin.httpAuth(`${server.baseUrl}${planned.plan.get_url}`);
    expect([fetched.status, Buffer.from(await fetched.arrayBuffer())]).toEqual([200, bytes]);
  }
  await expect(execution.download("review", "verdict", { attempt: 1 })).rejects.toMatchObject(INVALID);

  const as = (credential: string) => verify.Execution.connect(id, { client: WyrdClient.connect({ credential }) });
  const exact = await as(
    server.scopedApiKey("held_exact", [{ resource: "verifier", action: "read", scope: { verifier } }]),
  );
  expect(await exact.publications()).toEqual({ execution_id: id, items: listed, next_cursor: null });
  const viewer = await as(server.bootstrapService(["viewer"], "held_viewer"));
  await expect(viewer.cancel()).rejects.toMatchObject(DENIED);
  expect((await execution.invocations()).status).toBe("running");

  for (let repeat = 0; repeat < 2; repeat += 1) {
    expect(await execution.cancel()).toEqual({ execution_id: id, status: "cancelled" });
  }
  expect((await execution.publications()).items).toEqual(listed);
});
