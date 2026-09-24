import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
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
  readonly result: { execution_status: string; verdict: string; details: string | null };
  readonly features: string[];
}

/** Distribution signal over the baseline for `features`, indented for a Verifier spec. */
function distribution(features: string): string {
  return `      signal:
        kind: Distribution
        baseline_ref:
          kind: Data
          name: ts-edge-data
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
        weco_rule:
          rule_string: "8 16 4 8 2 4 1 1"
        alert_threshold: Zone1
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

/** Write and register the Parquet baseline Data Card. */
async function registerBaseline(cards: Cards, root: string): Promise<void> {
  const bytes = readFileSync(BASELINE_PARQUET);
  mkdirSync(join(root, "data"), { recursive: true });
  writeFileSync(join(root, "data/data.parquet"), bytes);
  const hex = createHash("sha256").update(bytes).digest("hex");
  const digest = createHash("sha256").update(bytes).digest("base64");
  const path = join(root, "baseline.yaml");
  writeFileSync(
    path,
    `apiVersion: wyrd/v1
kind: Data
metadata:
  name: ts-edge-data
  version: 1.0.0
  space: default
spec:
  interface:
    kind: Parquet
    meta:
      compression: Snappy
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

/** Write and register one Drift Verifier from {@link VERIFIERS}. */
async function registerVerifier(cards: Cards, root: string, name: string): Promise<CardRef> {
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
${VERIFIERS[name]}`,
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
    `SELECT execution_status, verdict, details
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

/** Assert a result completed inconclusive before scoring: null details, no features. */
function assertUnscored({ result, features }: Outcome): void {
  expect([result.execution_status, result.verdict]).toEqual(["completed", "inconclusive"]);
  expect(result.details).toBeNull();
  expect(features).toEqual([]);
}

describe("drift method edge journey", () => {
  it("scores each method's edge cases through the production runtime", async () => {
    const root = mkdtempSync(join(tmpdir(), "wyrd-ts-drift-"));
    const server = startTestServer(true, true);
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
        const runId = await verification.startRun(request);
        const deadline = Date.now() + WAIT_MS;
        let status = await verification.getRun(runId);
        while (["pending", "running", "retrying"].includes(status.status)) {
          expect(Date.now() < deadline, `run never settled: ${JSON.stringify(status)}`).toBe(true);
          await new Promise((resolve) => setTimeout(resolve, 100));
          status = await verification.getRun(runId);
        }
        expect(status.status, JSON.stringify(status)).toBe("completed");
        expect(status.dispatches, "a direct run never dispatches").toEqual([]);
        return readResult(server, query, status.result_id ?? "");
      };

      const custom = await registerVerifier(cards, root, "ts-edge-custom");
      const steady = await subject(cards, root, "ts-edge-steady");
      assertUnscored(await run(custom, steady));

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
      outcome = await run(custom, steady);
      expect(outcome.result.verdict, "a mean at the threshold is no drift").toBe("passed");
      expect(outcome.features).toEqual(["score/Custom/no_drift"]);

      const calm = await subject(cards, root, "ts-edge-calm");
      const nearCenter = [...Array(5).fill(45.0), ...Array(5).fill(54.0), ...Array(3).fill(49.0)];
      await emitRows(
        server,
        cards,
        calm,
        join(bundles, "calm"),
        nearCenter.map((latency: number) => ({ latency, tier: "gold", score: 1.0 })),
      );
      outcome = await run(spc, calm);
      expect(outcome.result.verdict, "two chunks and a trailing chunk pass").toBe("passed");
      expect(outcome.features).toEqual(["latency/Spc/no_drift"]);

      const sparse = await subject(cards, root, "ts-edge-sparse");
      await emitRows(server, cards, sparse, join(bundles, "sparse"), baselineLike.slice(0, 3));
      outcome = await run(psi, sparse);
      expect(outcome.result.verdict).toBe("inconclusive");
      expect(outcome.features).toEqual(["latency/Psi/inconclusive", "tier/Psi/inconclusive"]);
      outcome = await run(spc, sparse);
      expect(outcome.result.verdict).toBe("inconclusive");
      expect(outcome.features).toEqual(["latency/Spc/inconclusive"]);

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
