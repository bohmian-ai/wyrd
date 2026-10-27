import { describe, expect, test } from "vitest";

import * as wyrd from "wyrd-ai";
import { Observe, verify } from "wyrd-ai";

type NativeRun = Parameters<typeof Observe.fromNative>[0];

/** One call the fake native run recorded, as the arguments it received. */
type Call = readonly unknown[];

/**
 * A fake native run that records its arguments and answers fixed envelopes.
 *
 * These tests cover the wrapper's input projection, refusals, and result
 * projection. The real-server journeys cover execution.
 */
function fakeRun(result: Record<string, unknown> = { valueJson: "null" }, submission: Record<string, unknown> = {}) {
  const calls: Call[] = [];
  const answer =
    (value: unknown) =>
    async (...args: Call) => {
      calls.push(args);
      return value;
    };
  const native = { record: answer(result), verify: answer(result), submit: answer(submission) };
  return { observe: Observe.fromNative(native as unknown as NativeRun), calls };
}

/** Every JSON-carrying method, called with `value`, so a refusal must hold for each. */
function everyCall(observe: Observe, value: () => unknown): (() => Promise<unknown>)[] {
  return [
    () => observe.record("bifrost.datasets.events", value()),
    () => observe.verify("policy-check", { answer: value() as never }),
    () => observe.submit("policy-check", { answer: value() as never }),
  ];
}

/** The JSON payload a call sent native, whichever method recorded it. */
function payload(call: Call): unknown {
  return call[0] === "bifrost.datasets.events" ? call[1] : (call[1] as { valueJson: string }[])[0].valueJson;
}

// Internal: these drive the TypeScript wrapper over a fake native run, which
// no user constructs; the real-server journeys prove the public path.
describe("internal native run wrapper", () => {
  test("a refusal from native raises its projected catalog error", async () => {
    const { observe } = fakeRun({
      errorCode: "WYRD_SDK_404_UNKNOWN_VERIFIER",
      errorStatus: 404,
      errorTitle: "Unknown Verifier",
      errorDetail: "no Verifier named policy-check is bound",
    });

    await expect(observe.verify("policy-check", { answer: "yes" })).rejects.toMatchObject({
      code: "WYRD_SDK_404_UNKNOWN_VERIFIER",
      status: 404,
    });
  });

  test.each([
    ["null", null],
    ["undefined", undefined],
    ["an Error", new Error("customer-secret@example.test is not a number")],
    ["malformed issues", { issues: "customer-secret@example.test" }],
    ["issues at a returned key", { issues: [{ path: ["outer", "customer-secret@example.test"] }] }],
  ])("a parser rejecting with %s raises the data error at the safe root", async (_, thrown) => {
    const data = { outer: { "customer-secret@example.test": "wrong" } };
    const result = {
      execution_id: "0190f5e8-7c1a-7000-8000-000000000001",
      verifier: { kind: "Verifier", name: "judge", version: "1.0.0" },
      subject: { kind: "Agent", name: "drafter", version: "1.0.0" },
      outcome: {
        verdict: "failed",
        requirements: [{ id: "keyed", statement: "s", outcome: "failed", reason: "r", data }],
        steps: [],
        evidence: {},
      },
    };
    const { observe, calls } = fakeRun({ valueJson: JSON.stringify(result) });
    const parser = {
      parse(): never {
        throw thrown;
      },
    };

    const error = await observe.verifyTyped("judge", { answer: "yes" }, parser).then(
      () => undefined,
      (refused: unknown) => refused,
    );

    expect(error).toBeInstanceOf(wyrd.WyrdError);
    const refused = error as wyrd.WyrdError;
    expect(refused.code).toBe("WYRD_SDK_422_VERIFICATION_DATA_INVALID");
    expect(refused.details).toEqual({ requirement: "keyed", path: "" });
    expect(JSON.stringify(refused.details) + refused.message).not.toContain("customer-secret");
    expect(calls).toHaveLength(1);
  });

  test("slot values cross as JSON, lazy files, and Artifact references", async () => {
    const { observe, calls } = fakeRun();
    const bytes = new Uint8Array([0, 37, 80, 68, 70]).subarray(1);

    await observe.verify("policy-check", {
      answer: { text: "yes" },
      policy: verify.File.path("/does/not/exist/policy.pdf"),
      deck: verify.File.bytes(bytes, { format: "pdf", filename: "deck.pdf" }),
      archive: new verify.ArtifactRef({ kind: "Artifact", name: "decks", version: "1.0.0" }, "q3.pdf"),
    });

    const [verifier, inputs] = calls[0] as [string, Record<string, unknown>[]];
    expect(verifier).toBe("policy-check");
    expect(inputs[0]).toEqual({ slot: "answer", valueJson: '{"text":"yes"}' });
    expect(inputs[1]).toMatchObject({ slot: "policy", file: { path: "/does/not/exist/policy.pdf" } });
    const deck = inputs[2] as { file: { bytes: Buffer; format: string; filename: string } };
    expect([...deck.file.bytes]).toEqual([37, 80, 68, 70]);
    expect(deck.file).toMatchObject({ format: "pdf", filename: "deck.pdf" });
    expect(inputs[3]).toEqual({
      slot: "archive",
      artifact: '{"kind":"Artifact","name":"decks","version":"1.0.0"}',
      artifactPath: "q3.pdf",
    });
  });

  test("a lone non-record value, or an object marked positional, crosses without a slot", async () => {
    const { observe, calls } = fakeRun();
    const artifact = { kind: "Artifact", name: "decks", version: "1.0.0" } as const;

    await observe.verify("answer-check", "yes");
    await observe.verify("answer-check", 7);
    await observe.verify("answer-check", null);
    await observe.verify("answer-check", ["a", 1]);
    await observe.verify("answer-check", verify.File.path("/does/not/exist/policy.pdf"));
    await observe.verify("answer-check", new verify.ArtifactRef(artifact));
    await observe.verify("answer-check", { answer: "yes" }, { positional: true });
    await observe.submit("answer-check", { answer: "yes" }, { positional: true });
    await observe.verify("answer-check", { answer: "yes" });

    const sent = calls.map((call) => (call[1] as Record<string, unknown>[])[0]);
    expect(sent.slice(0, 4)).toEqual([
      { slot: undefined, valueJson: '"yes"' },
      { slot: undefined, valueJson: "7" },
      { slot: undefined, valueJson: "null" },
      { slot: undefined, valueJson: '["a",1]' },
    ]);
    expect(sent[4]).toMatchObject({ slot: undefined, file: { path: "/does/not/exist/policy.pdf" } });
    expect(sent[5]).toMatchObject({ slot: undefined, artifact: JSON.stringify(artifact) });
    expect(sent[6]).toEqual({ slot: undefined, valueJson: '{"answer":"yes"}' });
    expect(sent[7]).toEqual({ slot: undefined, valueJson: '{"answer":"yes"}' });
    expect(sent[8]).toEqual({ slot: "answer", valueJson: '"yes"' });
    expect(calls.every((call) => (call[1] as unknown[]).length === 1)).toBe(true);
  });

  test("constructing a File reads nothing", () => {
    expect(verify.File.path("/does/not/exist.pdf").path).toBe("/does/not/exist.pdf");
  });

  test("submit returns Recorded for samples and a queued handle for Run evidence", async () => {
    const recorded = fakeRun();
    expect(await recorded.observe.submit("latency-watch", { latency: 840 })).toEqual({ kind: "recorded" });

    const queued = fakeRun({}, { execution: { id: "01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff" } });
    const submission = await queued.observe.submit("policy-check", { answer: "yes" });
    expect(submission.kind === "queued" && submission.execution.id).toBe("01890f28-7c4a-7cc3-98e7-4f4a3c2d1bff");

    const refused = fakeRun({}, { error: { errorCode: "WYRD_VERIFICATION_400_INPUT_INVALID", errorStatus: 400 } });
    await expect(refused.observe.submit("policy-check", { missing: 1 })).rejects.toMatchObject({
      code: "WYRD_VERIFICATION_400_INPUT_INVALID",
    });
  });

  test("verification values live only under the verify namespace", () => {
    expect(Object.keys(verify).sort()).toEqual(["ArtifactRef", "Execution", "File"]);
    for (const name of ["ArtifactRef", "Execution", "File", "VerificationExecution"]) {
      expect(name in wyrd).toBe(false);
    }
  });

  test("the observation emits are gone", () => {
    expect("drift" in Observe.prototype).toBe(false);
    expect("eval" in Observe.prototype).toBe(false);
  });

  /** A value that `JSON.stringify` would silently drop. */
  const hidden = () => Object.defineProperty({ visible: 1 }, "hidden", { value: 2, enumerable: false });
  const extra = () => Object.assign([1], { extra: 2 });
  const symbolArray = () => Object.assign([1], { [Symbol("k")]: 2 });
  const cycle = () => {
    const self: Record<string, unknown> = {};
    self.self = self;
    return self;
  };

  test.for<readonly [string, () => unknown]>([
    ["undefined root", () => undefined],
    ["function root", () => () => 1],
    ["nested undefined", () => ({ a: undefined })],
    ["nested function", () => ({ a: () => 1 })],
    ["nested symbol", () => ({ a: Symbol("s") })],
    ["root symbol key", () => ({ a: 1, [Symbol("k")]: 2 })],
    ["nested symbol key", () => ({ a: { b: 1, [Symbol("k")]: 2 } })],
    ["root non-enumerable property", hidden],
    ["nested non-enumerable property", () => ({ a: hidden() })],
    ["root array non-index property", extra],
    ["nested array non-index property", () => ({ a: extra() })],
    ["root array symbol key", symbolArray],
    ["nested array symbol key", () => ({ a: symbolArray() })],
    ["bigint", () => ({ a: 1n })],
    ["NaN", () => ({ a: Number.NaN })],
    ["Infinity", () => ({ a: Number.POSITIVE_INFINITY })],
    ["unsafe integer", () => ({ a: 2 ** 53 })],
    ["array hole", () => ({ a: [1, undefined] })],
    ["Map", () => ({ a: new Map([["k", 1]]) })],
    ["Date", () => ({ a: new Date(0) })],
    ["cycle", cycle],
  ])("%s is refused before native", async ([, value]) => {
    const { observe, calls } = fakeRun();

    for (const call of everyCall(observe, value)) {
      await expect(Promise.resolve().then(call)).rejects.toMatchObject({ code: "WYRD_SPEC_400_VALIDATION" });
    }
    expect(calls).toEqual([]);
  });

  /** Counts reads of an accessor that answers `first` once, then `undefined`. */
  function readOnce(first: unknown) {
    const counter = { reads: 0 };
    const descriptor = { enumerable: true, get: () => (counter.reads++ === 0 ? first : undefined) };
    return { counter, descriptor };
  }

  test.for<readonly [string, (descriptor: PropertyDescriptor) => unknown, unknown]>([
    ["root getter", (d) => Object.defineProperty({}, "a", d), { a: "yes" }],
    ["nested getter", (d) => ({ b: Object.defineProperty({}, "a", d) }), { b: { a: "yes" } }],
    ["array accessor", (d) => Object.defineProperty([0], 0, d), ["yes"]],
    ["nested array accessor", (d) => ({ b: Object.defineProperty([0], 0, d) }), { b: ["yes"] }],
  ])("%s is read once and native gets its first value", async ([, input, expected]) => {
    const { observe, calls } = fakeRun();
    const accessors = [readOnce("yes"), readOnce("yes"), readOnce("yes")];
    let next = 0;

    for (const [index, call] of everyCall(observe, () => input(accessors[next++].descriptor)).entries()) {
      await call();
      expect(accessors[index].counter.reads).toBe(1);
    }
    expect(calls.map(payload)).toEqual(Array(3).fill(JSON.stringify(expected)));
  });

  test("accessor whose first value is unsupported is read once and refused", async () => {
    const { observe, calls } = fakeRun();

    for (const call of everyCall(observe, () => ({ b: Object.defineProperty({}, "a", readOnce(1n).descriptor) }))) {
      await expect(Promise.resolve().then(call)).rejects.toMatchObject({ code: "WYRD_SPEC_400_VALIDATION" });
    }
    expect(calls).toEqual([]);
  });
});
