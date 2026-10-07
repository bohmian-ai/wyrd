import { TraceFlags, context, trace } from "@opentelemetry/api";
import { afterAll, beforeAll, expect, test } from "vitest";

import { type EvalMediaRef, Observe } from "@wyrd/sdk";

import { StorageContextManager } from "../support/otel-context.js";

type NativeRun = Parameters<typeof Observe.fromNative>[0];

/** One emit the fake native run recorded, as the arguments it received. */
type Emit = readonly (string | undefined)[];

/**
 * A fake native run that records its arguments and answers one envelope.
 *
 * These tests cover the wrapper's refusals, error projection, and span capture.
 * Integration readback in `observe-a-run` covers the happy path.
 */
function fakeRun(result: Record<string, unknown> = { valueJson: "null" }) {
  const emits: Emit[] = [];
  const record = (...args: Emit) => (emits.push(args), result);
  const native = { drift: record, eval: record, record: async (...args: Emit) => record(...args) };
  return { observe: Observe.fromNative(native as unknown as NativeRun), emits };
}

/** Every emit method, called with `value`, so a refusal must hold for each. */
function everyEmit(observe: Observe, value: () => unknown): (() => unknown)[] {
  return [
    () => observe.drift(value() as Record<string, number>),
    () => observe.eval(value()),
    () => observe.record("vala.datasets.events", value()),
  ];
}

/** The JSON payload an emit sent native, whichever method recorded it. */
function payload(emit: Emit): string | undefined {
  return emit[0] === "vala.datasets.events" ? emit[1] : emit[0];
}

/** The trace and span ids an `eval` emit sent native. */
function ids(emit: Emit | undefined): [string | undefined, string | undefined] {
  return [emit?.[3], emit?.[4]];
}

test("emit refused by native raises its projected catalog error", () => {
  const { observe } = fakeRun({
    errorCode: "WYRD_SDK_400_BIFROST_NOT_STARTED",
    errorStatus: 400,
    errorTitle: "Bifrost is not started",
    errorDetail: "call start_bifrost before observing",
  });

  expect(() => observe.drift({ score: 1 })).toThrow(
    expect.objectContaining({ code: "WYRD_SDK_400_BIFROST_NOT_STARTED", status: 400 }),
  );
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
  const { observe, emits } = fakeRun();

  for (const emit of everyEmit(observe, value)) {
    await expect(Promise.resolve().then(emit)).rejects.toMatchObject({ code: "WYRD_SPEC_400_VALIDATION" });
  }
  expect(emits).toEqual([]);
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
  const { observe, emits } = fakeRun();
  const accessors = [readOnce("yes"), readOnce("yes"), readOnce("yes")];
  const values = accessors.map(({ descriptor }) => () => input(descriptor));

  for (const [index, emit] of everyEmit(observe, () => values.shift()!()).entries()) {
    await emit();
    expect(accessors[index]!.counter.reads).toBe(1);
  }
  expect(emits.map(payload)).toEqual(Array(3).fill(JSON.stringify(expected)));
});

test("accessor whose first value is unsupported is read once and refused", async () => {
  const { observe, emits } = fakeRun();

  for (const emit of everyEmit(observe, () => ({ b: Object.defineProperty({}, "a", readOnce(1n).descriptor) }))) {
    await expect(Promise.resolve().then(emit)).rejects.toMatchObject({ code: "WYRD_SPEC_400_VALIDATION" });
  }
  expect(emits).toEqual([]);
});

const PAGE = { id: "page", kind: "document", uri: "s3://bucket/page.pdf" } as const;

test.for<readonly [string, object]>([
  ["undeclared key", { ...PAGE, extra: 1 }],
  ["symbol key", { ...PAGE, [Symbol("k")]: 1 }],
  ["non-enumerable key", Object.defineProperty({ ...PAGE }, "mediaType", { value: "a/b" })],
  ["non-string uri", { ...PAGE, uri: Symbol("u") }],
])("media descriptor with a %s is refused before native", ([, item]) => {
  const { observe, emits } = fakeRun();

  expect(() => observe.eval({ n: 1 }, { media: [item as EvalMediaRef] })).toThrow(
    expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }),
  );
  expect(emits).toEqual([]);
});

test("media descriptor accessor is read once", () => {
  const { observe, emits } = fakeRun();
  const { counter, descriptor } = readOnce("application/pdf");

  observe.eval({ n: 1 }, { media: [Object.defineProperty({ ...PAGE }, "mediaType", descriptor) as EvalMediaRef] });

  expect(counter.reads).toBe(1);
  expect(emits.map((emit) => emit[2])).toEqual([JSON.stringify([{ ...PAGE, media_type: "application/pdf" }])]);
});

const ACTIVE = { traceId: "4bf92f3577b34da6a3ce929d0e0e4736", spanId: "00f067aa0ba902b7" };
const EXPLICIT = { traceId: "0af7651916cd43dd8448eb211c80319c", spanId: "b7ad6b7169203331" };
const INVALID = { traceId: "00000000000000000000000000000000", spanId: "0000000000000000" };

/** Run `emit` with a span holding `span`'s ids active, the way an instrumented app would. */
function within(span: typeof ACTIVE, emit: () => void): void {
  const flags = span === INVALID ? TraceFlags.NONE : TraceFlags.SAMPLED;
  context.with(trace.setSpan(context.active(), trace.wrapSpanContext({ ...span, traceFlags: flags })), emit);
}

beforeAll(() => {
  context.setGlobalContextManager(new StorageContextManager());
});
afterAll(() => {
  context.disable();
});

test("active span supplies both ids", () => {
  const { observe, emits } = fakeRun();

  observe.eval({ n: 0 });
  within(ACTIVE, () => observe.eval({ n: 1 }));

  expect(emits.map(ids)).toEqual([[undefined, undefined], [ACTIVE.traceId, ACTIVE.spanId]]);
});

test("explicit ids win over the active span", () => {
  const { observe, emits } = fakeRun();

  within(ACTIVE, () => observe.eval({ n: 2 }, EXPLICIT));

  expect(ids(emits[0])).toEqual([EXPLICIT.traceId, EXPLICIT.spanId]);
});

test("invalid active span supplies no ids", () => {
  const { observe, emits } = fakeRun();

  within(INVALID, () => observe.eval({ n: 3 }));

  expect(ids(emits[0])).toEqual([undefined, undefined]);
});
