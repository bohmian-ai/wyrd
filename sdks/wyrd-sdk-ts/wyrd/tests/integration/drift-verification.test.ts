import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type NativeWyrdTestServer, startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import {
  Bifrost,
  type CardRef,
  Cards,
  type StartVerificationRunRequest,
  Verification,
  WyrdError,
  WyrdState,
} from "@wyrd/sdk";

/** Upper bound on every wait for the server-side runtime to make progress. */
const WAIT_MS = 90_000;

/** Rows in the baseline Parquet fixture: `latency` 0..99, `tier` gold/silver alternating. */
const BASELINE_ROWS = 100;

/** Committed Parquet baseline; TypeScript has no in-tree Parquet writer. */
const BASELINE_PARQUET = fileURLToPath(
  new URL("../fixtures/drift-baseline.parquet", import.meta.url),
);

/** One result summary and its `feature/method/verdict` rows. */
interface Outcome {
  readonly result: {
    execution_status: string;
    verdict: string;
    details: string | null;
    owner_card_uid: string | null;
    binding_id: string | null;
    subject_card_uid: string;
  };
  readonly features: string[];
}

/** Interface block of the committed Parquet baseline. */
const PARQUET_INTERFACE = `    kind: Parquet
    meta:
      compression: Snappy`;

/** Distribution signal over `baseline` for `features`, indented for a Verifier spec. */
function distribution(features: string, baseline = "ts-edge-data"): string {
  return `      signal:
        kind: Distribution
        baseline_ref:
          kind: Data
          name: ${baseline}
          version: 1.0.0
          space: default
        features: [${features}]
      condition:
        kind: Statistical
`;
}

/** Method blocks of the PSI, SPC, and Custom Verifiers the journey registers. */
const VERIFIERS: Record<string, string> = {
  "ts-edge-psi": `      method: Psi
${distribution("latency, tier")}      profile:
        kind: Psi
        binning_strategy:
          kind: EqualWidth
          n_bins: 10
        categorical_features: [tier]
        threshold:
          kind: Fixed
          value: 0.25
`,
  "ts-edge-spc": `      method: Spc
${distribution("latency")}      profile:
        kind: Spc
        sample_size: 5
`,
  "ts-edge-custom": `      method: Custom
      signal:
        kind: Metric
        name: score
      condition:
        kind: Statistical
      profile:
        kind: Custom
        metric_name: score
        baseline_value: 1.0
        alert_threshold: 0.5
`,
};

/**
 * Write and register the baseline bytes as Data Card `name` declaring `iface`.
 *
 * The default declares the committed Parquet file as Parquet; another
 * interface registers the same bytes as a Data Card no Drift baseline accepts.
 */
async function registerBaseline(
  cards: Cards,
  root: string,
  name = "ts-edge-data",
  iface = PARQUET_INTERFACE,
): Promise<void> {
  const bytes = readFileSync(BASELINE_PARQUET);
  mkdirSync(join(root, "data"), { recursive: true });
  writeFileSync(join(root, "data/data.parquet"), bytes);
  const hex = createHash("sha256").update(bytes).digest("hex");
  const digest = createHash("sha256").update(bytes).digest("base64");
  const path = join(root, `${name}.yaml`);
  writeFileSync(
    path,
    `apiVersion: wyrd/v1
kind: Data
metadata:
  name: ${name}
  version: 1.0.0
  space: default
spec:
  interface:
${iface}
  schema:
    columns:
      - name: latency
        dtype: float64
      - name: tier
        dtype: string
  card_refs: []
  stats:
    row_count: ${BASELINE_ROWS}
    col_count: 2
    byte_count: ${bytes.length}
    sha256: ${hex}
artifacts:
  - relative_path: data/data.parquet
    sha256: ${digest}
    size_bytes: ${bytes.length}
    content_type: application/vnd.apache.parquet
`,
  );
  await cards.registerFromPath(path);
}

/** Write and register one Drift Verifier from {@link VERIFIERS}, or with method block `body`. */
async function registerVerifier(
  cards: Cards,
  root: string,
  name: string,
  body = VERIFIERS[name],
): Promise<CardRef> {
  const path = join(root, `${name}.yaml`);
  writeFileSync(
    path,
    `apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: ${name}
  version: 1.0.0
  space: default
spec:
  implementation:
    kind: drift
    spec:
${body}`,
  );
  return (await cards.registerFromPath(path)).root;
}

/** Register an unbound Service named `name` as a Drift subject. */
async function subject(cards: Cards, root: string, name: string): Promise<CardRef> {
  const path = join(root, `${name}.yaml`);
  writeFileSync(
    path,
    `apiVersion: wyrd/v1
kind: Service
metadata:
  name: ${name}
  version: 1.0.0
  space: default
spec: {}
`,
  );
  return (await cards.registerFromPath(path)).root;
}

/** Register a Service bound to the SPC Verifier that dispatches one Operator on failure. */
async function boundSubject(cards: Cards, root: string): Promise<CardRef> {
  const path = join(root, "ts-edge-bound.yaml");
  writeFileSync(
    path,
    `apiVersion: wyrd/v1
kind: Service
metadata:
  name: ts-edge-bound
  version: 1.0.0
  space: default
spec:
  verified_by:
    - verifier:
        kind: Verifier
        name: ts-edge-spc
        version: 1.0.0
        space: default
      runs_on:
        kind: schedule
        cron: "0 0 * * *"
      on_failure:
        - kind: http
          method: post
          url: https://hooks.example.test/ts-drift
`,
  );
  return (await cards.registerFromPath(path)).root;
}

/** Poll `runId` as the subject until it leaves the active states, and require `expected`. */
async function settle(
  server: NativeWyrdTestServer,
  service: CardRef,
  runId: string,
  expected = "completed",
): Promise<Awaited<ReturnType<Verification["getRun"]>>> {
  const verification = Verification.connect({
    serverUrl: server.baseUrl,
    credential: subjectCredential(server, service),
  });
  const deadline = Date.now() + WAIT_MS;
  let status = await verification.getRun(runId);
  while (["pending", "running", "retrying"].includes(status.status)) {
    expect(Date.now() < deadline, `run never settled: ${JSON.stringify(status)}`).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 100));
    status = await verification.getRun(runId);
  }
  expect(status.status, JSON.stringify(status)).toBe(expected);
  return status;
}

/**
 * Make `bindingId` due and return the one run its occurrence schedules.
 *
 * Dueness is PostgreSQL's decision, so the test harness places the schedule
 * cursor at statement time and the verification runtime schedules the
 * occurrence; the daily window `[midnight UTC, now)` holds this journey's rows.
 */
async function scheduledRun(server: NativeWyrdTestServer, bindingId: string): Promise<string> {
  const earlier = new Set(server.verificationRuns());
  server.makeBindingDue(bindingId);
  const deadline = Date.now() + WAIT_MS;
  let runs = server.verificationRuns().filter((run) => !earlier.has(run));
  while (runs.length === 0) {
    expect(Date.now() < deadline, "the due binding never scheduled a run").toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 100));
    runs = server.verificationRuns().filter((run) => !earlier.has(run));
  }
  expect(runs).toHaveLength(1);
  return runs[0] ?? "";
}

/** Issue the subject Service's own key, whose Card scope covers its manual runs. */
function subjectCredential(server: NativeWyrdTestServer, service: CardRef): string {
  return server.credentialRegisteredService(
    `${service.space}/Service/${service.name}@${service.version}`,
    ["admin"],
  );
}

/** Poll a Verifier's Card status until its fitted baseline is ready. */
async function waitReady(cards: Cards, verifier: CardRef): Promise<void> {
  const deadline = Date.now() + WAIT_MS;
  for (;;) {
    const baseline = (await cards.get(verifier)).status?.verification?.baseline;
    expect(baseline, `${verifier.name} serves baseline status`).toBeDefined();
    expect(["pending", "building", "ready"]).toContain(baseline?.state);
    if (baseline?.state === "ready") {
      return;
    }
    expect(Date.now() < deadline, `${verifier.name} never fitted`).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/**
 * Emit `rows` as Drift observations of `service` through one `WyrdState` lifetime.
 *
 * Each lifetime is one client batch; the batch is drained and flushed before returning.
 */
async function emitRows(
  server: NativeWyrdTestServer,
  cards: Cards,
  service: CardRef,
  bundle: string,
  rows: readonly Record<string, string | number | boolean>[],
): Promise<void> {
  await cards.hydrate(service, bundle);
  const state = WyrdState.fromPath(bundle);
  await state.startBifrost({
    serverUrl: server.baseUrl,
    credential: subjectCredential(server, service),
    grpcUrl: server.grpcUrl,
  });
  const run = state.run();
  for (const row of rows) {
    run.observe.drift(row);
  }
  await state.shutdown();
  server.flushBifrost();
}

/** Materialize every row of one query as plain objects. */
async function rows(query: Bifrost, sql: string): Promise<Record<string, unknown>[]> {
  return (await query.sql(sql))
    .toArrow()
    .toArray()
    .map((row) => row.toJSON());
}

/**
 * Flush Scribe, then read one result and its feature rows.
 *
 * A tenant that has never scored a report has no feature table yet, which reads
 * as no feature rows.
 */
async function readResult(
  server: NativeWyrdTestServer,
  query: Bifrost,
  resultId: string,
): Promise<Outcome> {
  server.flushBifrost();
  const results = await rows(
    query,
    `SELECT execution_status, verdict, details, owner_card_uid, binding_id, subject_card_uid
       FROM vala.verification.results WHERE result_id = '${resultId}'`,
  );
  expect(results).toHaveLength(1);
  let features: Record<string, unknown>[] = [];
  try {
    features = await rows(
      query,
      `SELECT f.feature, f.method, f.verdict FROM vala.drift.result_features f
         JOIN vala.verification.results r ON f.result_id = r.result_id
         WHERE r.result_id = '${resultId}' ORDER BY f.feature`,
    );
  } catch (error) {
    expect(error).toBeInstanceOf(WyrdError);
    expect((error as WyrdError).code).toBe("WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND");
  }
  return {
    result: results[0] as Outcome["result"],
    features: features.map((row) => `${row.feature}/${row.method}/${row.verdict}`),
  };
}

/**
 * Assert the persisted SPC evidence of `latency` in a result's `details`.
 *
 * The baseline's twenty subgroups of five consecutive integers fix the X-bar
 * center at 49.5 and the S center at `sqrt(2.5)`; the limits must be the NIST
 * X-bar/S limits around them, and SPC compares signals with zero.
 */
function assertSpcEvidence(details: string | null, subgroups: number, xBarSignals: number): void {
  const feature = JSON.parse(details ?? "null").features.latency;
  const spc = feature.evidence.Spc;
  expect([spc.subgroup_size, spc.subgroups], JSON.stringify(spc)).toEqual([5, subgroups]);
  expect(spc.x_bar.signals).toBe(xBarSignals);
  const sBar = Math.sqrt(2.5);
  expect(spc.x_bar.center).toBeCloseTo(49.5, 9);
  expect(spc.x_bar.upper).toBeCloseTo(49.5 + (3 * sBar) / (0.9399856 * Math.sqrt(5)), 5);
  expect(spc.s.center).toBeCloseTo(sBar, 9);
  expect(spc.s.lower, "B3 is zero for subgroups of five").toBe(0);
  expect(feature.threshold).toBe(0);
}

/**
 * Assert the persisted PSI bin evidence of a baseline-like window in `details`.
 *
 * The window mirrors the baseline's 100 rows: `tier` lands 50/50 in its two
 * labeled bins with nothing in the reserved unseen-label bin, and every
 * `latency` row lands in exactly one equal-width bin.
 */
function assertPsiEvidence(details: string | null): void {
  const features = JSON.parse(details ?? "null").features;
  const tier = features.tier.evidence.Psi;
  expect(tier.sample, JSON.stringify(tier)).toBe(BASELINE_ROWS);
  expect(
    tier.bins.map((bin: { bin: { categorical_value?: unknown }; target_count: number }) => [
      bin.bin.categorical_value ?? null,
      bin.target_count,
    ]),
  ).toEqual([
    ["gold", BASELINE_ROWS / 2],
    ["silver", BASELINE_ROWS / 2],
    [null, 0],
  ]);
  const latency = features.latency.evidence.Psi;
  expect(latency.sample).toBe(BASELINE_ROWS);
  expect(
    latency.bins.reduce((sum: number, bin: { target_count: number }) => sum + bin.target_count, 0),
  ).toBe(BASELINE_ROWS);
}

/** Await a promise expected to reject with a structured catalog error. */
async function rejection(promise: Promise<unknown>): Promise<WyrdError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  return error as WyrdError;
}

/** Assert a result completed inconclusive before scoring: null details, no features. */
function assertUnscored({ result, features }: Outcome): void {
  expect([result.execution_status, result.verdict]).toEqual(["completed", "inconclusive"]);
  expect(result.details).toBeNull();
  expect(features).toEqual([]);
}

describe("drift method edge journey", () => {
  it("scores each method's edge cases through the production runtime", async () => {
    const root = mkdtempSync(join(tmpdir(), "wyrd-ts-drift-"));
    const server = startTestServer(undefined, true, true);
    try {
      const cards = Cards.connect({ serverUrl: server.baseUrl, credential: server.apiKey });
      const query = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
        grpcUrl: server.grpcUrl,
      });
      const now = Date.now();
      const start = new Date(now - 3_600_000);
      const end = new Date(now + 3_600_000);

      /** Run `verifier` directly over `service` for `[from, to)` as the subject and read it. */
      const run = async (
        verifier: CardRef,
        service: CardRef,
        from: Date = start,
        to: Date = end,
      ): Promise<Outcome> => {
        const verification = Verification.connect({
          serverUrl: server.baseUrl,
          credential: subjectCredential(server, service),
        });
        const request: StartVerificationRunRequest = {
          target: {
            kind: "verifier",
            verifier_uid: verifier.uid ?? "",
            subject_card_uid: service.uid ?? "",
          },
          input: { kind: "drift_window", start: from.toISOString(), end: to.toISOString() },
        };
        const status = await settle(server, service, await verification.startRun(request));
        expect(status.dispatches, "a direct run never dispatches").toEqual([]);
        expect(status.requested_by_principal_id).toBeTruthy();
        const outcome = await readResult(server, query, status.result_id ?? "");
        expect(
          [outcome.result.owner_card_uid, outcome.result.binding_id, outcome.result.subject_card_uid],
          "a direct result names its subject but no owner or binding",
        ).toEqual([null, null, service.uid]);
        return outcome;
      };

      const custom = await registerVerifier(cards, root, "ts-edge-custom");
      const steady = await subject(cards, root, "ts-edge-steady");
      assertUnscored(await run(custom, steady));

      // Registration refuses the retired Drift and Eval Card kinds visibly.
      for (const kind of ["Drift", "Eval"]) {
        const path = join(root, `retired-${kind}.yaml`);
        writeFileSync(
          path,
          `apiVersion: wyrd/v1
kind: ${kind}
metadata:
  name: ts-retired-${kind.toLowerCase()}
  version: 1.0.0
  space: default
spec: {}
`,
        );
        const refused = await rejection(cards.registerFromPath(path));
        expect(refused.code, `kind: ${kind} is not registrable`).toBe(
          "WYRD_REGISTRY_400_INVALID_CARD_SPEC",
        );
        expect(JSON.stringify(refused.details)).toContain("WYRD_LOADER_400_INVALID_ENVELOPE");
      }
      // A Drift baseline over a non-Parquet Data Card is refused at registration.
      await registerBaseline(
        cards,
        root,
        "ts-edge-ipc",
        `    kind: Arrow
    meta:
      format: Ipc
      framework_version: 17.0.0`,
      );
      const ipc = await rejection(
        registerVerifier(
          cards,
          root,
          "ts-edge-ipc-psi",
          VERIFIERS["ts-edge-psi"]?.replace(
            distribution("latency, tier"),
            distribution("latency, tier", "ts-edge-ipc"),
          ),
        ),
      );
      expect(ipc.code, "a non-Parquet baseline is refused").toBe("WYRD_DRIFT_400_VALIDATION");

      await registerBaseline(cards, root);
      const psi = await registerVerifier(cards, root, "ts-edge-psi");
      const spc = await registerVerifier(cards, root, "ts-edge-spc");
      await waitReady(cards, psi);
      await waitReady(cards, spc);

      const bundles = join(root, "bundles");
      const baselineLike = Array.from({ length: BASELINE_ROWS }, (_, row) => ({
        latency: (row * 37) % BASELINE_ROWS,
        tier: row % 2 === 0 ? "gold" : "silver",
        score: row % 2 === 0 ? 1.0 : 2.0,
      }));
      await emitRows(server, cards, steady, join(bundles, "steady"), baselineLike);
      let outcome = await run(psi, steady);
      expect(outcome.result.verdict).toBe("passed");
      expect(outcome.features).toEqual(["latency/Psi/no_drift", "tier/Psi/no_drift"]);
      assertPsiEvidence(outcome.result.details);
      const outsider = await rejection(
        Verification.connect({
          serverUrl: server.baseUrl,
          credential: server.scopedApiKey("ts_edge_reader", ["cards:read"]),
        }).startRun({
          target: { kind: "verifier", verifier_uid: psi.uid ?? "", subject_card_uid: steady.uid ?? "" },
          input: { kind: "drift_window", start: start.toISOString(), end: end.toISOString() },
        }),
      );
      expect(outsider.status, "a principal without run permission is refused").toBe(403);
      outcome = await run(custom, steady);
      expect(outcome.result.verdict, "a mean at the threshold is no drift").toBe("passed");
      expect(outcome.features).toEqual(["score/Custom/no_drift"]);

      const latencies = (values: number[]) =>
        values.map((latency) => ({ latency, tier: "gold", score: 1.0 }));
      const calm = await subject(cards, root, "ts-edge-calm");
      await emitRows(server, cards, calm, join(bundles, "calm"), latencies([48, 49, 50, 51, 52, 48, 49, 50, 51, 52]));
      outcome = await run(spc, calm);
      expect(outcome.result.verdict, "two in-control subgroups pass").toBe("passed");
      expect(outcome.features).toEqual(["latency/Spc/no_drift"]);
      assertSpcEvidence(outcome.result.details, 2, 0);
      await emitRows(server, cards, calm, join(bundles, "calm-partial"), latencies([50, 50]));
      assertUnscored(await run(spc, calm));

      const sparse = await subject(cards, root, "ts-edge-sparse");
      await emitRows(server, cards, sparse, join(bundles, "sparse"), baselineLike.slice(0, 3));
      assertUnscored(await run(psi, sparse));
      assertUnscored(await run(spc, sparse));

      const gappy = await subject(cards, root, "ts-edge-gappy");
      await emitRows(server, cards, gappy, join(bundles, "gappy"), baselineLike);
      await emitRows(server, cards, gappy, join(bundles, "gappy-unrelated"), Array(5).fill({ score: 9.0 }));
      expect(
        (await run(psi, gappy)).result.verdict,
        "records without a PSI feature do not enter PSI",
      ).toBe("passed");
      await emitRows(server, cards, gappy, join(bundles, "gappy-omitted"), [{ latency: 50.0 }]);
      assertUnscored(await run(psi, gappy));

      const bound = await boundSubject(cards, root);
      const bindingIds = (await cards.get(bound)).status?.verification?.binding_ids;
      expect(bindingIds).toHaveLength(1);
      await emitRows(server, cards, bound, join(bundles, "bound"), latencies(Array(20).fill(200)));
      const bindingRun = await Verification.connect({
        serverUrl: server.baseUrl,
        credential: subjectCredential(server, bound),
      }).startRun({
        target: { kind: "binding", binding_id: bindingIds?.[0] ?? "" },
        input: { kind: "drift_window", start: start.toISOString(), end: end.toISOString() },
      });
      const failed = await settle(server, bound, bindingRun);
      const scheduled = await settle(server, bound, await scheduledRun(server, bindingIds?.[0] ?? ""));
      for (const status of [failed, scheduled]) {
        expect(status.dispatches, "a failed binding result dispatches its Operator").toHaveLength(1);
        outcome = await readResult(server, query, status.result_id ?? "");
        expect([outcome.result.owner_card_uid, outcome.result.binding_id]).toEqual([
          bound.uid,
          bindingIds?.[0],
        ]);
        expect(outcome.result.verdict).toBe("failed");
        expect(outcome.features).toEqual(["latency/Spc/drift"]);
        assertSpcEvidence(outcome.result.details, 4, 4);
      }

      server.retireFittedFormat(spc.uid ?? "");
      const refused = await settle(
        server,
        calm,
        await Verification.connect({
          serverUrl: server.baseUrl,
          credential: subjectCredential(server, calm),
        }).startRun({
          target: { kind: "verifier", verifier_uid: spc.uid ?? "", subject_card_uid: calm.uid ?? "" },
          input: { kind: "drift_window", start: start.toISOString(), end: end.toISOString() },
        }),
        "errored",
      );
      expect(refused.error?.code, JSON.stringify(refused)).toBe("baseline_legacy");
      expect(refused.result_id ?? null, "a refused legacy run is never scored").toBeNull();
      outcome = await readResult(server, query, failed.result_id ?? "");
      expect(outcome.result.verdict, "the historical result stays readable").toBe("failed");
      assertSpcEvidence(outcome.result.details, 4, 4);

      const retired = join(root, "ts-edge-weco.yaml");
      writeFileSync(
        retired,
        readFileSync(join(root, "ts-edge-spc.yaml"), "utf8")
          .replace("ts-edge-spc", "ts-edge-weco")
          .replace("sample_size: 5\n", 'sample_size: 5\n        weco_rule:\n          rule_string: "8 16 4 8 2 4 1 1"\n'),
      );
      const legacy = await cards.registerFromPath(retired).then(
        () => undefined,
        (reason: unknown) => reason,
      );
      expect(legacy).toBeInstanceOf(WyrdError);
      expect(JSON.stringify((legacy as WyrdError).details)).toContain("weco_rule");

      const weighted = await subject(cards, root, "ts-edge-weighted");
      const scores = (values: number[]) =>
        values.map((score) => ({ latency: 50.0, tier: "gold", score }));
      await emitRows(server, cards, weighted, join(bundles, "weighted-a"), scores([1.0]));
      const split = new Date();
      await emitRows(server, cards, weighted, join(bundles, "weighted-b"), scores([2.0, 2.0, 2.0]));
      expect(
        (await run(custom, weighted)).result.verdict,
        "rows average 1.75; batches would average 1.5",
      ).toBe("failed");
      expect(
        (await run(custom, weighted, start, split)).result.verdict,
        "the window end excludes the second batch",
      ).toBe("passed");
      expect(
        (await run(custom, weighted, split, end)).result.verdict,
        "the window start excludes the first batch",
      ).toBe("failed");

      const text = await subject(cards, root, "ts-edge-text");
      await emitRows(server, cards, text, join(bundles, "text"), Array(3).fill({ score: "high" }));
      assertUnscored(await run(custom, text));
    } finally {
      server.shutdown();
    }
  }, 300_000);
});

/** One request a local mock received: its path and parsed JSON body. */
interface Received {
  readonly path: string;
  readonly body: Record<string, unknown>;
}

/**
 * Start a loopback HTTP server answering every POST with `answer`, recording
 * each request it served; returns its root URL, records, and a close hook.
 */
async function listen(
  answer: (path: string) => { status: number; body?: unknown },
): Promise<{ url: string; received: Received[]; close: () => Promise<void> }> {
  const received: Received[] = [];
  const server: Server = createServer((request: IncomingMessage, response: ServerResponse) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      const text = Buffer.concat(chunks).toString();
      const path = request.url ?? "";
      received.push({ path, body: text ? (JSON.parse(text) as Record<string, unknown>) : {} });
      const { status, body } = answer(path);
      response.writeHead(status, { "content-type": "application/json" });
      response.end(body === undefined ? undefined : JSON.stringify(body));
    });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address() as AddressInfo;
  return {
    url: `http://127.0.0.1:${port}`,
    received,
    close: () =>
      new Promise<void>((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      ),
  };
}

/** JSON schema of the judge's structured answer. */
const JUDGE_SCHEMA = {
  type: "object",
  properties: { passed: { type: "boolean" } },
  required: ["passed"],
  additionalProperties: false,
};

/** Chat Completions answer the local judge provider returns: the judge passes. */
const JUDGE_COMPLETION = {
  id: "chatcmpl-ts-judge",
  object: "chat.completion",
  created: 1,
  model: "gpt-test",
  choices: [
    {
      index: 0,
      finish_reason: "stop",
      message: { role: "assistant", content: '{"passed":true}' },
    },
  ],
  usage: { prompt_tokens: 5, completion_tokens: 3, total_tokens: 8 },
};

/** A Drift observation as an application declares it. */
type LatencyFeatures = { latency: number; tier: string; score: number };

/** An Eval context as an application declares it. */
type Exchange = { question: string; answer: string };

/**
 * Write the continuous Service graph and return its path.
 *
 * The Service composes a Model bound to the registered PSI, SPC, and Custom
 * Drift Verifiers and an Agent bound to a deterministic and an LLM-judge Eval
 * Verifier. SPC and the deterministic Eval dispatch an inline HTTP Operator to
 * `hook` when they fail.
 */
function writeContinuousGraph(root: string, hook: string): string {
  const drift = (name: string, operator = "") => `        - verifier:
            kind: Verifier
            name: ${name}
            version: 1.0.0
            space: default
          runs_on:
            kind: schedule
            cron: "0 0 * * *"
${operator}`;
  const onFailure = (path: string, indent: string) => `${indent}on_failure:
${indent}  - kind: http
${indent}    method: post
${indent}    url: ${hook}${path}
`;
  const evalVerifier = (name: string, spec: string) => `apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: ${name}
  version: 1.0.0
  space: default
spec:
  implementation:
    kind: eval
    spec:
${spec}`;
  const files: Record<string, string> = {
    "judge-prompt.json": JSON.stringify({
      apiVersion: "wyrd/v1",
      kind: "Prompt",
      metadata: { name: "ts-judge-prompt", version: "1.0.0", space: "default" },
      spec: {
        request: {
          provider: "open_ai_chat_completion",
          body: {
            model: "gpt-test",
            messages: [{ role: "user", content: "Grade the answer ${answer}." }],
            response_format: {
              type: "json_schema",
              json_schema: { name: "judge_result", schema: JUDGE_SCHEMA },
            },
          },
        },
        model: "gpt-test",
        variables: ["answer"],
        media_variables: [],
        response_type: { json_schema: { name: "judge_result", schema: JUDGE_SCHEMA } },
      },
    }),
    "agent-prompt.yaml": `apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: ts-agent-prompt
  version: 1.0.0
  space: default
spec:
  provider: openai
  model: gpt-test
  messages: [answer the question]
`,
    "agent.yaml": `apiVersion: wyrd/v1
kind: Agent
metadata:
  name: ts-continuous-agent
  version: 1.0.0
  space: default
spec:
  prompt: ./agent-prompt.yaml
  run_config:
    max_iterations: 1
`,
    "model.yaml": `apiVersion: wyrd/v1
kind: Model
metadata:
  name: ts-continuous-model
  version: 1.0.0
  space: default
spec:
  interface:
    kind: Custom
    meta:
      framework_version: 0.1.0
      loader_module: fixture
      loader_class: TinyModel
      extra: {}
  task_type: Other
  signature:
    inputs:
      - name: input
        dtype: float64
    outputs:
      - name: output
        dtype: float64
  card_refs: []
`,
    "eval-assert.yaml": evalVerifier(
      "ts-eval-assert",
      `      pass_gate: {kind: all_pass}
      tasks:
        answer: {kind: assertion, id: answer, context_path: $.answer, operator: equals, expected: "yes"}
`,
    ),
    "eval-judge.yaml": evalVerifier(
      "ts-eval-judge",
      `      pass_gate: {kind: all_pass}
      tasks:
        judge:
          kind: llm_judge
          id: judge
          judge_ref: {prompt: ./judge-prompt.json, tool_names: [], run_config: {max_iterations: 1}}
          context_path: $.answer
          operator: equals
          expected: {passed: true}
          max_retries: 0
`,
    ),
    "service.yaml": `apiVersion: wyrd/v1
kind: Service
metadata:
  name: ts-continuous-service
  version: 1.0.0
  space: default
spec:
  service_type: agent
  components:
    - alias: model
      ref: ./model.yaml
      verified_by:
${drift("ts-edge-psi")}${drift("ts-edge-spc", onFailure("/spc", "          "))}${drift("ts-edge-custom")}    - alias: agent
      ref: ./agent.yaml
      verified_by:
        - verifier: ./eval-assert.yaml
          runs_on: {kind: observations_ready}
${onFailure("/eval", "          ")}        - verifier: ./eval-judge.yaml
          runs_on: {kind: observations_ready}
`,
  };
  for (const [name, body] of Object.entries(files)) {
    writeFileSync(join(root, name), body);
  }
  return join(root, "service.yaml");
}

/** Poll until `count` runs exist beyond `earlier` and return them, oldest first. */
async function newRuns(
  server: NativeWyrdTestServer,
  earlier: ReadonlySet<string>,
  count: number,
): Promise<string[]> {
  const deadline = Date.now() + WAIT_MS;
  let runs = server.verificationRuns().filter((run) => !earlier.has(run));
  while (runs.length < count) {
    expect(Date.now() < deadline, `only ${runs.length} of ${count} runs were enqueued`).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 100));
    runs = server.verificationRuns().filter((run) => !earlier.has(run));
  }
  expect(runs).toHaveLength(count);
  return runs;
}

/** Poll `runId` as `service` until every Operator dispatch is delivered. */
async function delivered(
  server: NativeWyrdTestServer,
  service: CardRef,
  runId: string,
): Promise<Awaited<ReturnType<Verification["getRun"]>>> {
  const verification = Verification.connect({
    serverUrl: server.baseUrl,
    credential: subjectCredential(server, service),
  });
  const deadline = Date.now() + WAIT_MS;
  let status = await verification.getRun(runId);
  while (status.dispatches.some((dispatch) => dispatch.status !== "delivered")) {
    expect(
      status.dispatches.every((dispatch) => dispatch.status !== "failed"),
      JSON.stringify(status),
    ).toBe(true);
    expect(Date.now() < deadline, `dispatch never delivered: ${JSON.stringify(status)}`).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 100));
    status = await verification.getRun(runId);
  }
  return status;
}

describe("continuous verification journey", () => {
  it("verifies one Service through Drift, Eval, and an Operator end to end", async () => {
    const root = mkdtempSync(join(tmpdir(), "wyrd-ts-continuous-"));
    const judge = await listen((path) =>
      path === "/v1/chat/completions"
        ? { status: 200, body: JUDGE_COMPLETION }
        : { status: 404, body: { error: path } },
    );
    const hook = await listen(() => ({ status: 204 }));
    const server = startTestServer(judge.url, true, true);
    try {
      const cards = Cards.connect({ serverUrl: server.baseUrl, credential: server.apiKey });
      const query = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: server.apiKey,
        grpcUrl: server.grpcUrl,
      });
      await registerBaseline(cards, root);
      const verifiers = new Map<string, string>();
      for (const name of ["ts-edge-psi", "ts-edge-spc", "ts-edge-custom"]) {
        const verifier = await registerVerifier(cards, root, name);
        verifiers.set(verifier.uid ?? "", name);
        if (name !== "ts-edge-custom") {
          await waitReady(cards, verifier);
        }
      }

      const service = (await cards.registerFromPath(writeContinuousGraph(root, hook.url))).root;
      for (const name of ["ts-eval-assert", "ts-eval-judge"]) {
        verifiers.set((await cards.get(`default/Verifier/${name}@1.0.0`)).metadata.uid ?? "", name);
      }
      const bundle = join(root, "bundle");
      await cards.hydrate(service, bundle);

      const admin = Verification.connect({ serverUrl: server.baseUrl, credential: server.apiKey });
      const bindingIds = (await cards.get(service)).status?.verification?.binding_ids ?? [];
      const bindings = new Map<string, Awaited<ReturnType<Verification["getBinding"]>>>();
      for (const id of bindingIds) {
        const binding = await admin.getBinding(id);
        expect(binding.owner_card_uid).toBe(service.uid);
        expect(binding.active, "registration alone activates no binding").toBe(false);
        bindings.set(verifiers.get(binding.verifier_uid) ?? binding.verifier_uid, binding);
      }
      expect([...bindings.keys()].sort()).toEqual([
        "ts-edge-custom",
        "ts-edge-psi",
        "ts-edge-spc",
        "ts-eval-assert",
        "ts-eval-judge",
      ]);

      // The exact Card-bound principal authenticates the state; that activates
      // every binding the Service owns.
      const state = WyrdState.fromPath(bundle);
      await state.startBifrost({
        serverUrl: server.baseUrl,
        credential: subjectCredential(server, service),
        grpcUrl: server.grpcUrl,
      });
      for (const id of bindingIds) {
        expect((await admin.getBinding(id)).active, "the exact principal activated its owner").toBe(true);
      }

      // One invocation switches between the Model and Agent scopes and emits
      // both language-native typed objects and untyped plain objects.
      const run = state.run();
      const model = run.forCard("model");
      const agent = run.forCard("agent");
      expect([model.runId, agent.runId]).toEqual([run.runId, run.runId]);
      const earlier = new Set(server.verificationRuns());
      const modelUid = model.cardRef.split("#")[1];
      const agentUid = agent.cardRef.split("#")[1];
      expect(bindings.get("ts-edge-psi")?.subject_card_uid).toBe(modelUid);
      expect(bindings.get("ts-eval-assert")?.subject_card_uid).toBe(agentUid);
      for (let row = 0; row < BASELINE_ROWS; row += 1) {
        const latency = (row * 37) % BASELINE_ROWS;
        if (row % 2 === 0) {
          const features: LatencyFeatures = { latency, tier: "gold", score: 1.0 };
          model.observe.drift(features);
        } else {
          model.observe.drift({ latency, tier: "silver", score: 2.0 });
        }
      }
      const passing: Exchange = { question: "is the ledger canonical?", answer: "yes" };
      agent.observe.eval(passing);
      agent.observe.eval({ question: "is the ledger canonical?", answer: "no" });
      await state.shutdown();
      server.flushBifrost();

      // Each acknowledged Eval record enqueues one run per Eval binding, which
      // the runtime executes through the deterministic engine and the judge.
      const evalRuns = await newRuns(server, earlier, 4);
      const evalResults = [];
      for (const runId of evalRuns) {
        const status = await settle(server, service, runId);
        const outcome = await readResult(server, query, status.result_id ?? "");
        evalResults.push({ status, outcome });
      }
      server.flushBifrost();
      const evals = await rows(
        query,
        `SELECT record_id, context, card_uid, run_id FROM vala.eval.observations
           WHERE run_id = '${run.runId}'`,
      );
      expect(evals.map((row) => [row.card_uid, row.run_id])).toEqual([
        [agentUid, run.runId],
        [agentUid, run.runId],
      ]);
      const recordOf = new Map(
        evals.map((row) => [row.record_id, JSON.parse(String(row.context)).answer as string]),
      );
      const verdicts = await rows(
        query,
        `SELECT binding_id, source_record_id, verdict, owner_card_uid, subject_card_uid
           FROM vala.verification.results
           WHERE result_id IN (${evalResults.map(({ status }) => `'${status.result_id}'`).join(", ")})`,
      );
      const bindingName = (id: unknown) =>
        [...bindings.entries()].find(([, binding]) => binding.binding_id === id)?.[0];
      expect(
        verdicts
          .map((row) => {
            expect([row.owner_card_uid, row.subject_card_uid]).toEqual([service.uid, agentUid]);
            return `${bindingName(row.binding_id)}/${recordOf.get(row.source_record_id)}/${row.verdict}`;
          })
          .sort(),
      ).toEqual([
        "ts-eval-assert/no/failed",
        "ts-eval-assert/yes/passed",
        "ts-eval-judge/no/passed",
        "ts-eval-judge/yes/passed",
      ]);
      const judged = judge.received.filter((request) => request.path === "/v1/chat/completions");
      expect(judged, "the judge called the local provider once per record").toHaveLength(2);
      expect(JSON.stringify(judged.map((request) => request.body.messages)).includes("Grade the answer")).toBe(
        true,
      );
      const failedEval = evalResults.filter(({ status }) => status.dispatches.length > 0);
      expect(failedEval, "only the failed deterministic verdict dispatches").toHaveLength(1);

      // Each Drift binding runs manually over the window as the Service.
      const verification = Verification.connect({
        serverUrl: server.baseUrl,
        credential: subjectCredential(server, service),
      });
      const now = Date.now();
      const window = {
        kind: "drift_window",
        start: new Date(now - 3_600_000).toISOString(),
        end: new Date(now + 3_600_000).toISOString(),
      } as const;
      const drifted = new Map<string, Awaited<ReturnType<Verification["getRun"]>>>();
      for (const name of ["ts-edge-psi", "ts-edge-spc", "ts-edge-custom"]) {
        const binding = bindings.get(name);
        const status = await settle(
          server,
          service,
          await verification.startRun({
            target: { kind: "binding", binding_id: binding?.binding_id ?? "" },
            input: window,
          }),
        );
        expect(status.requested_by_principal_id).toBeTruthy();
        drifted.set(name, status);
      }
      let outcome = await readResult(server, query, drifted.get("ts-edge-psi")?.result_id ?? "");
      expect(outcome.result.verdict).toBe("passed");
      expect(outcome.features).toEqual(["latency/Psi/no_drift", "tier/Psi/no_drift"]);
      expect([outcome.result.owner_card_uid, outcome.result.binding_id, outcome.result.subject_card_uid]).toEqual(
        [service.uid, bindings.get("ts-edge-psi")?.binding_id, modelUid],
      );
      assertPsiEvidence(outcome.result.details);
      outcome = await readResult(server, query, drifted.get("ts-edge-spc")?.result_id ?? "");
      expect(outcome.result.verdict, "uniform latencies signal the X-bar chart").toBe("failed");
      expect(outcome.features).toEqual(["latency/Spc/drift"]);
      outcome = await readResult(server, query, drifted.get("ts-edge-custom")?.result_id ?? "");
      expect(outcome.result.verdict, "a mean at the threshold is no drift").toBe("passed");
      expect(outcome.features).toEqual(["score/Custom/no_drift"]);
      expect(drifted.get("ts-edge-psi")?.dispatches).toEqual([]);
      expect(drifted.get("ts-edge-custom")?.dispatches).toEqual([]);

      // Each failed verdict's HTTP Operator delivers to the local receiver,
      // and Run GET reports that delivery.
      for (const status of [failedEval[0]?.status, drifted.get("ts-edge-spc")]) {
        const settled = await delivered(server, service, status?.run_id ?? "");
        expect(settled.dispatches.map((dispatch) => dispatch.status)).toEqual(["delivered"]);
      }
      expect(hook.received.map((request) => request.path).sort()).toEqual(["/eval", "/spc"]);

      const drifts = await rows(
        query,
        `SELECT DISTINCT card_uid, run_id FROM vala.drift.observations WHERE run_id = '${run.runId}'`,
      );
      expect(drifts).toEqual([{ card_uid: modelUid, run_id: run.runId }]);

      // Unauthorized and cross-tenant callers are refused.
      const reader = Verification.connect({
        serverUrl: server.baseUrl,
        credential: server.scopedApiKey("ts_continuous_reader", ["cards:read"]),
      });
      const denied = await rejection(
        reader.startRun({
          target: { kind: "binding", binding_id: bindings.get("ts-edge-psi")?.binding_id ?? "" },
          input: window,
        }),
      );
      expect(denied.status).toBe(403);
      const foreignKey = server.bootstrapServiceInTenant(
        server.seedTenant("ts-continuous-other"),
        ["admin"],
        "ts-continuous-foreign",
      );
      const foreign = Verification.connect({ serverUrl: server.baseUrl, credential: foreignKey });
      expect((await rejection(foreign.getRun(drifted.get("ts-edge-spc")?.run_id ?? ""))).status).toBe(404);
      expect(
        (
          await rejection(
            foreign.startRun({
              target: { kind: "binding", binding_id: bindings.get("ts-edge-psi")?.binding_id ?? "" },
              input: window,
            }),
          )
        ).status,
      ).toBe(404);
      const foreignQuery = await Bifrost.connect({
        serverUrl: server.baseUrl,
        credential: foreignKey,
        grpcUrl: server.grpcUrl,
      });
      const foreignRows = await rows(
        foreignQuery,
        `SELECT result_id FROM vala.verification.results`,
      ).catch((error: unknown) => {
        expect((error as WyrdError).code).toBe("WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND");
        return [];
      });
      expect(foreignRows, "another tenant reads none of these results").toEqual([]);
    } finally {
      server.shutdown();
      await judge.close();
      await hook.close();
    }
  }, 300_000);
});
