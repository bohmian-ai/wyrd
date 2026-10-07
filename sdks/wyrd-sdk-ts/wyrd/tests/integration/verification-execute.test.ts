import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { type NativeWyrdTestServer, startTestServer } from "@wyrd/testing";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

import { type CardRef, Cards, type Judgment, WyrdError, WyrdState } from "@wyrd/sdk";

/** Upper bound on every wait for the verification runtime to fit a baseline. */
const WAIT_MS = 90_000;

/** Committed Parquet baseline: `latency` 0..99, `tier` gold/silver alternating. */
const BASELINE_PARQUET = fileURLToPath(
  new URL("../fixtures/drift-baseline.parquet", import.meta.url),
);

/** JSON schema of the judge's structured answer. */
const JUDGE_SCHEMA = {
  type: "object",
  properties: { passed: { type: "boolean" } },
  required: ["passed"],
  additionalProperties: false,
};

/** Chat Completions answer the local judge provider returns: the judge passes. */
const JUDGE_COMPLETION = {
  id: "chatcmpl-ts-execute",
  object: "chat.completion",
  created: 1,
  model: "gpt-test",
  choices: [
    { index: 0, finish_reason: "stop", message: { role: "assistant", content: '{"passed":true}' } },
  ],
  usage: { prompt_tokens: 5, completion_tokens: 3, total_tokens: 8 },
};

/** Distribution signal over Data Card `baseline` for `features`, indented for a Verifier spec. */
const distribution = (features: string, baseline = "ts-exec-data") => `      signal:
        kind: Distribution
        baseline_ref: {kind: Data, name: ${baseline}, version: 1.0.0, space: default}
        features: [${features}]
      condition:
        kind: Statistical
`;

/** SPC method block over `feature` of Data Card `baseline`. */
const spcBody = (feature = "latency", baseline = "ts-exec-data") => `      method: Spc
${distribution(feature, baseline)}      profile:
        kind: Spc
        sample_size: 5
`;

/** Drift method blocks of the Verifiers the journey registers. */
const DRIFT: Record<string, string> = {
  "ts-exec-psi": `      method: Psi
${distribution("latency, tier")}      profile:
        kind: Psi
        binning_strategy: {kind: EqualWidth, n_bins: 10}
        categorical_features: [tier]
        threshold: {kind: Fixed, value: 0.25}
`,
  "ts-exec-spc": spcBody(),
  "ts-exec-custom": `      method: Custom
      signal: {kind: Metric, name: score}
      condition: {kind: Statistical}
      profile: {kind: Custom, metric_name: score, baseline_value: 1.0, alert_threshold: 0.5}
`,
  // The baseline schema declares `ghost`, which its Parquet lacks, so the fit never becomes ready.
  "ts-exec-unfitted": spcBody("ghost", "ts-exec-ghost"),
};

/** LLM-judge task grading `$.answer` with the JSON-schema judge Prompt and no retries. */
const JUDGE_TASK = `        judge:
          kind: llm_judge
          id: judge
          judge_ref: {prompt: ./judge-prompt.json, tool_names: [], run_config: {max_iterations: 1}}
          context_path: $.answer
          operator: equals
          expected: {passed: true}
          max_retries: 0
`;

/** Judge answer the mock refuses with a provider failure. */
const BROKEN_ANSWER = "broken";

/** Eval task blocks of the Eval Verifiers the journey registers. */
const EVAL: Record<string, string> = {
  "ts-exec-assert": `        answer: {kind: assertion, id: answer, context_path: $.answer, operator: equals, expected: "yes"}
`,
  "ts-exec-judge": JUDGE_TASK,
  "ts-exec-traced": `        span: {kind: trace_assertion, id: span, span_selector: "$.spans[0].name", operator: equals, expected: x}
`,
};

/** Write and register `body` as `name.yaml` under `root`, returning the root Card. */
async function register(cards: Cards, root: string, name: string, body: string): Promise<CardRef> {
  const path = join(root, `${name}.yaml`);
  writeFileSync(path, body);
  return (await cards.registerFromPath(path)).root;
}

/** Register the committed Parquet bytes as Data Card `name`, optionally declaring extra columns. */
async function registerBaseline(cards: Cards, root: string, name: string, extra = ""): Promise<void> {
  const bytes = readFileSync(BASELINE_PARQUET);
  mkdirSync(join(root, "data"), { recursive: true });
  writeFileSync(join(root, "data/data.parquet"), bytes);
  const hex = createHash("sha256").update(bytes).digest("hex");
  const digest = createHash("sha256").update(bytes).digest("base64");
  await register(
    cards,
    root,
    name,
    `apiVersion: wyrd/v1
kind: Data
metadata: {name: ${name}, version: 1.0.0, space: default}
spec:
  interface:
    kind: Parquet
    meta: {compression: Snappy}
  schema:
    columns:
      - {name: latency, dtype: float64}
      - {name: tier, dtype: string}
${extra}  card_refs: []
  stats: {row_count: 100, col_count: 2, byte_count: ${bytes.length}, sha256: ${hex}}
artifacts:
  - relative_path: data/data.parquet
    sha256: ${digest}
    size_bytes: ${bytes.length}
    content_type: application/vnd.apache.parquet
`,
  );
}

/** Register a Verifier named `name` whose implementation is `kind` with `spec`. */
const verifier = (cards: Cards, root: string, name: string, kind: string, spec: string) =>
  register(
    cards,
    root,
    name,
    `apiVersion: wyrd/v1
kind: Verifier
metadata: {name: ${name}, version: 1.0.0, space: default}
spec:
  implementation:
    kind: ${kind}
    spec:
${spec}`,
  );

/** Poll a Verifier's Card status until its fitted baseline is ready. */
async function waitReady(cards: Cards, ref: CardRef): Promise<void> {
  const deadline = Date.now() + WAIT_MS;
  while ((await cards.get(ref)).status?.verification?.baseline?.state !== "ready") {
    expect(Date.now() < deadline, `${ref.name} never fitted`).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/** Await a promise expected to reject with a structured catalog error and return its code. */
async function refusal(promise: Promise<unknown>): Promise<string> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(WyrdError);
  return (error as WyrdError).code;
}

/** Registered targets, the subject's hydrated bundle, and the server every test executes against. */
interface Journey {
  readonly server: NativeWyrdTestServer;
  /** The subject's own key: its Card scope covers the subject it verifies. */
  readonly credential: string;
  readonly bundle: string;
  readonly subject: CardRef;
  readonly verifiers: Record<string, CardRef>;
  readonly close: () => Promise<void>;
}

/**
 * The subject Service binding every journey Verifier at its root: Drift on a
 * daily schedule and Eval on observations the journey never writes.
 */
const serviceYaml = (names: readonly string[]) => `apiVersion: wyrd/v1
kind: Service
metadata: {name: ts-exec-service, version: 1.0.0, space: default}
spec:
  verified_by:
${names
  .map(
    (name) => `    - verifier: {kind: Verifier, name: ${name}, version: 1.0.0, space: default}
      runs_on: ${name in EVAL ? "{kind: observations_ready}" : '{kind: schedule, cron: "0 0 * * *"}'}
`,
  )
  .join("")}`;

/** Drift rows of one feature `name` per value. */
const column = (name: string, values: readonly (number | string)[]) =>
  values.map((value) => ({ [name]: value }));

describe("direct verification journey", () => {
  let journey: Journey;

  /**
   * Judge `input` with Verifier `name` over the root subject as `credential`.
   *
   * Bifrost is not started, so `observe.verify` resolves its client from the
   * environment on first use; each call loads a fresh state so that client is
   * this credential's, and callers await one judgment before the next.
   */
  async function verifyAs(credential: string, name: string, input: unknown): Promise<Judgment> {
    vi.stubEnv("WYRD_SERVER_URL", journey.server.baseUrl);
    vi.stubEnv("WYRD_API_KEY", credential);
    vi.stubEnv("WYRD_ACCESS_TOKEN", undefined);
    vi.stubEnv("WYRD_WORKLOAD_TOKEN", undefined);
    return WyrdState.fromPath(journey.bundle).run().observe.verify(name, input);
  }

  beforeAll(async () => {
    const root = mkdtempSync(join(tmpdir(), "wyrd-ts-execute-"));
    // Routes on the graded answer in the request body, so one mock serves passing and broken judges.
    const judge: Server = createServer((req, res) => {
      const chunks: Buffer[] = [];
      req.on("data", (chunk: Buffer) => chunks.push(chunk));
      req.on("end", () => {
        const body = Buffer.concat(chunks).toString();
        const [status, answer] = body.includes(BROKEN_ANSWER)
          ? [500, { error: "provider failure" }]
          : [200, JUDGE_COMPLETION];
        res.writeHead(status, { "content-type": "application/json" });
        res.end(JSON.stringify(answer));
      });
    });
    await new Promise<void>((resolve) => judge.listen(0, "127.0.0.1", resolve));
    const server = startTestServer(
      `http://127.0.0.1:${(judge.address() as AddressInfo).port}`,
      true,
      true,
    );
    const cards = Cards.connect({ serverUrl: server.baseUrl, credential: server.apiKey });
    await registerBaseline(cards, root, "ts-exec-data");
    await registerBaseline(cards, root, "ts-exec-ghost", "      - {name: ghost, dtype: float64}\n");
    writeFileSync(
      join(root, "judge-prompt.json"),
      JSON.stringify({
        apiVersion: "wyrd/v1",
        kind: "Prompt",
        metadata: { name: "ts-exec-judge-prompt", version: "1.0.0", space: "default" },
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
    );
    const verifiers: Record<string, CardRef> = {};
    for (const [name, body] of Object.entries(DRIFT)) {
      verifiers[name] = await verifier(cards, root, name, "drift", body);
    }
    for (const [name, tasks] of Object.entries(EVAL)) {
      verifiers[name] = await verifier(
        cards,
        root,
        name,
        "eval",
        `      pass_gate: {kind: all_pass}\n      tasks:\n${tasks}`,
      );
    }
    for (const name of ["ts-exec-psi", "ts-exec-spc"]) {
      await waitReady(cards, verifiers[name] as CardRef);
    }
    const subject = await register(cards, root, "ts-exec-service", serviceYaml(Object.keys(verifiers)));
    const bundle = join(root, "bundle");
    await cards.hydrate(subject, bundle);
    journey = {
      server,
      credential: server.credentialRegisteredService("default/Service/ts-exec-service@1.0.0", ["admin"]),
      bundle,
      subject,
      verifiers,
      close: async () => {
        server.shutdown();
        judge.closeAllConnections();
        await new Promise<void>((resolve) => judge.close(() => resolve()));
      },
    };
  }, 180_000);

  afterAll(async () => {
    vi.unstubAllEnvs();
    await journey?.close();
  });

  it("judges supplied input with exact attribution and stable refusals", async () => {
    const verify = (name: string, input: unknown) => verifyAs(journey.credential, name, input);
    const baselineLike = Array.from({ length: 100 }, (_, row) => ({
      latency: (row * 37) % 100,
      tier: row % 2 === 0 ? "gold" : "silver",
    }));
    const cases: [string, unknown, Judgment["kind"], Judgment["verdict"]][] = [
      ["ts-exec-psi", baselineLike, "drift_psi", "passed"],
      ["ts-exec-psi", Array(100).fill({ latency: 99, tier: "gold" }), "drift_psi", "failed"],
      // The second row omits `latency`, a null sample that leaves PSI unscorable.
      ["ts-exec-psi", [{ latency: 1, tier: "gold" }, { tier: "silver" }], "drift_psi", "inconclusive"],
      ["ts-exec-spc", column("latency", [48, 49, 50, 51, 52, 48, 49, 50, 51, 52]), "drift_spc", "passed"],
      ["ts-exec-spc", column("latency", Array(20).fill(200)), "drift_spc", "failed"],
      ["ts-exec-custom", column("score", [1.0, 2.0]), "drift_custom", "passed"],
      ["ts-exec-custom", column("score", [2.0, 2.0]), "drift_custom", "failed"],
      ["ts-exec-custom", [{ score: 1.0 }, { tier: "gold" }], "drift_custom", "inconclusive"],
      ["ts-exec-assert", { answer: "yes" }, "eval_assertion", "passed"],
      ["ts-exec-assert", { answer: "no" }, "eval_assertion", "failed"],
      ["ts-exec-judge", { answer: "yes" }, "eval_llm_judge", "passed"],
    ];
    for (const [name, input, kind, verdict] of cases) {
      const judged = await verify(name, input);
      const label = `${name} ${verdict}: ${JSON.stringify(judged)}`;
      expect([judged.kind, judged.verdict, judged.passed], label).toEqual([kind, verdict, verdict === "passed"]);
      const registered = journey.verifiers[name];
      expect([judged.verifier.uid, judged.verifier.version], label).toEqual([
        registered?.uid,
        registered?.version,
      ]);
      expect([judged.subject.uid, judged.subject.version], label).toEqual([
        journey.subject.uid,
        journey.subject.version,
      ]);
      expect(judged.execution_id, label).toBeTruthy();
      expect(Object.keys(judged.detail), label).toEqual([kind.startsWith("drift") ? "drift" : "eval"]);
    }

    const wide = [Object.fromEntries(Array.from({ length: 65 }, (_, i) => [`c${i}`, 1.0]))];
    const refusals: [() => Promise<unknown>, string][] = [
      [() => verify("ts-exec-custom", column("score", [1.0, "a"])), "WYRD_VERIFICATION_400_INPUT_INVALID"],
      [() => verify("ts-exec-custom", wide), "WYRD_VERIFICATION_413_INPUT_TOO_LARGE"],
      [() => verify("ts-exec-custom", column("score", Array(100_001).fill(1))), "WYRD_VERIFICATION_413_INPUT_TOO_LARGE"],
      [() => verify("ts-exec-assert", { answer: "a".repeat(256 * 1024) }), "WYRD_VERIFICATION_413_INPUT_TOO_LARGE"],
      [() => verify("ts-exec-unfitted", column("ghost", [1.0])), "WYRD_VERIFICATION_409_BASELINE_NOT_READY"],
      [() => verify("ts-exec-custom", { answer: "yes" }), "WYRD_SDK_400_INVALID_OBSERVATION"],
      [() => verify("ts-exec-psi", column("latency", [1.0])), "WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE"],
      [() => verify("ts-exec-assert", { question: "?" }), "WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE"],
      [() => verify("ts-exec-traced", { answer: "yes" }), "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED"],
      [() => verify("ts-exec-judge", { answer: BROKEN_ANSWER }), "WYRD_VERIFICATION_502_DEPENDENCY_FAILED"],
      [() => verify("ts-exec-missing", { answer: "yes" }), "WYRD_SDK_404_UNKNOWN_VERIFIER"],
      [
        () =>
          verifyAs(journey.server.scopedApiKey("ts_exec_reader", ["cards:read"]), "ts-exec-assert", {
            answer: "yes",
          }),
        "WYRD_PERMISSION_403_DENIED_RBAC",
      ],
      [
        () =>
          verifyAs(
            journey.server.bootstrapServiceInTenant(
              journey.server.seedTenant("ts-exec-other"),
              ["admin"],
              "ts-exec-foreign",
            ),
            "ts-exec-assert",
            { answer: "yes" },
          ),
        "WYRD_VERIFICATION_404_TARGET_NOT_FOUND",
      ],
    ];
    const codes = [];
    // Sequential: each call resolves its client from the environment it stubs.
    for (const [call] of refusals) {
      codes.push(await refusal(call()));
    }
    expect(codes).toEqual(refusals.map(([, code]) => code));
  }, 120_000);
});
