import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type NativeWyrdTestServer, startTestServer } from "@wyrd/testing";
import { describe, expect, it } from "vitest";

import { Bifrost, type CardRef, Cards, WyrdError, WyrdState } from "@wyrd/sdk";

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

/** Issue the subject Service's own key, whose Card scope covers its observations and judgments. */
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

/**
 * Flush Scribe and query until `sql` returns `count` rows, then return them.
 *
 * A table no result has reached yet reads as no rows.
 */
async function rowsWhenCommitted(
  server: NativeWyrdTestServer,
  query: Bifrost,
  sql: string,
  count: number,
): Promise<Record<string, unknown>[]> {
  const deadline = Date.now() + WAIT_MS;
  for (;;) {
    server.flushBifrost();
    const found = await rows(query, sql).catch((error: unknown) => {
      expect((error as WyrdError).code).toBe("WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND");
      return [];
    });
    if (found.length >= count) {
      expect(found, sql).toHaveLength(count);
      return found;
    }
    expect(Date.now() < deadline, `only ${found.length} of ${count} rows committed: ${sql}`).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/** Poll until `receiver` has received `count` requests and return their sorted paths. */
async function receivedPaths(receiver: { received: Received[] }, count: number): Promise<string[]> {
  const deadline = Date.now() + WAIT_MS;
  while (receiver.received.length < count) {
    expect(Date.now() < deadline, `only ${receiver.received.length} of ${count} requests arrived`).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return receiver.received.map((request) => request.path).sort();
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
      for (const name of ["ts-edge-psi", "ts-edge-spc", "ts-edge-custom"]) {
        const verifier = await registerVerifier(cards, root, name);
        if (name !== "ts-edge-custom") {
          await waitReady(cards, verifier);
        }
      }

      const service = (await cards.registerFromPath(writeContinuousGraph(root, hook.url))).root;
      const bundle = join(root, "bundle");
      await cards.hydrate(service, bundle);
      const bindingIds = (await cards.get(service)).status?.verification?.binding_ids ?? [];
      expect(bindingIds, "the Service owns five bindings").toHaveLength(5);

      const state = WyrdState.fromPath(bundle);
      await state.startBifrost({
        serverUrl: server.baseUrl,
        credential: subjectCredential(server, service),
        grpcUrl: server.grpcUrl,
      });

      // One invocation switches between the Model and Agent scopes and emits
      // both language-native typed objects and untyped plain objects.
      const run = state.run();
      const model = run.forCard("model");
      const agent = run.forCard("agent");
      expect([model.runId, agent.runId]).toEqual([run.runId, run.runId]);
      expect([model.alias, agent.alias]).toEqual(["model", "agent"]);
      const modelUid = state.cardRef("model").uid;
      const agentUid = state.cardRef("agent").uid;
      const baselineLike: LatencyFeatures[] = [];
      for (let row = 0; row < BASELINE_ROWS; row += 1) {
        const latency = (row * 37) % BASELINE_ROWS;
        if (row % 2 === 0) {
          const features: LatencyFeatures = { latency, tier: "gold", score: 1.0 };
          model.observe.drift(features);
          baselineLike.push(features);
        } else {
          model.observe.drift({ latency, tier: "silver", score: 2.0 });
          baselineLike.push({ latency, tier: "silver", score: 2.0 });
        }
      }
      const passing: Exchange = { question: "is the ledger canonical?", answer: "yes" };
      agent.observe.eval(passing);
      agent.observe.eval({ question: "is the ledger canonical?", answer: "no" });

      // The Model view judges the same window directly with its bound PSI Verifier.
      const judged = await model.observe.verify("ts-edge-psi", baselineLike);
      expect([judged.kind, judged.verdict, judged.passed]).toEqual(["drift_psi", "passed", true]);
      expect([judged.subject.uid, judged.verifier.name]).toEqual([modelUid, "ts-edge-psi"]);
      await state.shutdown();
      server.flushBifrost();

      // Each acknowledged Eval record runs once per Eval binding, through the
      // deterministic engine and the judge; the task identifies the Verifier.
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
      const verdicts = await rowsWhenCommitted(
        server,
        query,
        `SELECT r.binding_id, r.source_record_id, r.verdict, r.owner_card_uid, r.subject_card_uid, i.task_id
           FROM vala.verification.results r JOIN vala.eval.result_items i ON r.result_id = i.result_id
           WHERE r.subject_card_uid = '${agentUid}'`,
        4,
      );
      const verifierOfTask: Record<string, string> = { answer: "ts-eval-assert", judge: "ts-eval-judge" };
      expect(
        verdicts
          .map((row) => {
            expect([row.owner_card_uid, row.subject_card_uid]).toEqual([service.uid, agentUid]);
            return `${verifierOfTask[String(row.task_id)]}/${recordOf.get(row.source_record_id)}/${row.verdict}`;
          })
          .sort(),
      ).toEqual([
        "ts-eval-assert/no/failed",
        "ts-eval-assert/yes/passed",
        "ts-eval-judge/no/passed",
        "ts-eval-judge/yes/passed",
      ]);
      const evalBindings = new Map(verdicts.map((row) => [row.binding_id, row.task_id]));
      expect(evalBindings.size, "each Eval binding runs one task for both records").toBe(2);
      for (const row of verdicts) {
        expect(evalBindings.get(row.binding_id)).toBe(row.task_id);
      }
      const judgedByProvider = judge.received.filter((request) => request.path === "/v1/chat/completions");
      expect(judgedByProvider, "the judge called the local provider once per record").toHaveLength(2);
      expect(
        JSON.stringify(judgedByProvider.map((request) => request.body.messages)).includes("Grade the answer"),
      ).toBe(true);

      // Each due Drift binding runs over its daily window `[midnight UTC, now)`,
      // which holds this journey's rows.
      const driftBindings = bindingIds.filter((id) => !evalBindings.has(id));
      expect(driftBindings).toHaveLength(3);
      for (const id of driftBindings) {
        server.makeBindingDue(id);
      }
      const drifted = new Map<string, Outcome>();
      for (const row of await rowsWhenCommitted(
        server,
        query,
        `SELECT result_id FROM vala.verification.results
           WHERE binding_id IN (${driftBindings.map((id) => `'${id}'`).join(", ")})`,
        3,
      )) {
        const outcome = await readResult(server, query, String(row.result_id));
        expect([outcome.result.owner_card_uid, outcome.result.subject_card_uid]).toEqual([
          service.uid,
          modelUid,
        ]);
        expect(driftBindings).toContain(outcome.result.binding_id);
        drifted.set(outcome.features.join(","), outcome);
      }
      expect([...drifted.keys()].sort()).toEqual([
        "latency/Psi/no_drift,tier/Psi/no_drift",
        "latency/Spc/drift",
        "score/Custom/no_drift",
      ]);
      const psi = drifted.get("latency/Psi/no_drift,tier/Psi/no_drift");
      expect(psi?.result.verdict).toBe("passed");
      assertPsiEvidence(psi?.result.details ?? null);
      expect(drifted.get("latency/Spc/drift")?.result.verdict, "uniform latencies signal the X-bar chart").toBe(
        "failed",
      );
      expect(drifted.get("score/Custom/no_drift")?.result.verdict, "a mean at the threshold is no drift").toBe(
        "passed",
      );

      // Only the failed deterministic Eval and the failed SPC verdict deliver
      // their HTTP Operator to the local receiver.
      expect(await receivedPaths(hook, 2)).toEqual(["/eval", "/spc"]);

      const drifts = await rows(
        query,
        `SELECT DISTINCT card_uid, run_id FROM vala.drift.observations WHERE run_id = '${run.runId}'`,
      );
      expect(drifts).toEqual([{ card_uid: modelUid, run_id: run.runId }]);

      // Another tenant reads none of these results.
      const foreignKey = server.bootstrapServiceInTenant(
        server.seedTenant("ts-continuous-other"),
        ["admin"],
        "ts-continuous-foreign",
      );
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
      // An SPC profile carrying the retired `weco_rule` field is refused.
      const retired = join(root, "ts-edge-weco.yaml");
      writeFileSync(
        retired,
        readFileSync(join(root, "ts-edge-spc.yaml"), "utf8")
          .replace("ts-edge-spc", "ts-edge-weco")
          .replace("sample_size: 5\n", 'sample_size: 5\n        weco_rule:\n          rule_string: "8 16 4 8 2 4 1 1"\n'),
      );
      const legacy = await rejection(cards.registerFromPath(retired));
      expect(JSON.stringify(legacy.details)).toContain("weco_rule");
    } finally {
      server.shutdown();
      await judge.close();
      await hook.close();
    }
  }, 300_000);
});
