import { TraceFlags, context, trace } from "@opentelemetry/api";
import { describe, expect, it } from "vitest";

import { type EvalMediaRef, Observe, WyrdError } from "@wyrd/sdk";

import { StorageContextManager } from "../support/otel-context.js";

type NativeRun = ConstructorParameters<typeof Observe>[0];

/** One emit the fake native run recorded, as the arguments it received. */
type Emit = readonly (string | undefined)[];

/**
 * A fake native run that records its arguments and answers one envelope.
 *
 * The wrapper's whole job is argument conversion and error projection, so a
 * recorder proves it without the napi addon or a hydrated bundle.
 */
function fakeRun(result: Record<string, unknown> = { valueJson: "null" }): {
  readonly native: NativeRun;
  readonly emits: Emit[];
} {
  const emits: Emit[] = [];
  const native = {
    drift(...args: Emit) {
      emits.push(args);
      return result;
    },
    eval(...args: Emit) {
      emits.push(args);
      return result;
    },
    async record(...args: Emit) {
      emits.push(args);
      return result;
    },
  } as unknown as NativeRun;
  return { native, emits };
}

const ACTIVE_TRACE = "4bf92f3577b34da6a3ce929d0e0e4736";
const ACTIVE_SPAN = "00f067aa0ba902b7";

/** Run `emit` with a valid span active, the way an instrumented app would. */
function withActiveSpan(emit: () => void): void {
  const span = trace.wrapSpanContext({
    traceId: ACTIVE_TRACE,
    spanId: ACTIVE_SPAN,
    traceFlags: TraceFlags.SAMPLED,
  });
  context.with(trace.setSpan(context.active(), span), emit);
}

describe("Observe", () => {
  it("passes the feature map as JSON text with its session", () => {
    const { native, emits } = fakeRun();
    new Observe(native).drift(
      { latency_ms: 12.5, tier: "gold" },
      { sessionId: "00000000-0000-0000-0000-00000000000a" },
    );
    expect(emits).toEqual([
      [
        JSON.stringify({ latency_ms: 12.5, tier: "gold" }),
        "00000000-0000-0000-0000-00000000000a",
      ],
    ]);
  });

  it("encodes media descriptors with their canonical field names", () => {
    const { native, emits } = fakeRun();
    new Observe(native).eval(
      { answer: "yes" },
      {
        media: [
          { id: "page", kind: "document", uri: "s3://bucket/page.pdf", mediaType: "application/pdf" },
          { id: "shot", kind: "image", uri: "s3://bucket/shot.png" },
        ],
        traceId: "4bf92f3577b34da6a3ce929d0e0e4736",
        spanId: "00f067aa0ba902b7",
      },
    );
    expect(emits).toEqual([
      [
        JSON.stringify({ answer: "yes" }),
        undefined,
        JSON.stringify([
          {
            id: "page",
            kind: "document",
            uri: "s3://bucket/page.pdf",
            media_type: "application/pdf",
          },
          { id: "shot", kind: "image", uri: "s3://bucket/shot.png" },
        ]),
        "4bf92f3577b34da6a3ce929d0e0e4736",
        "00f067aa0ba902b7",
      ],
    ]);
  });

  it("omits media entirely when the caller names none", () => {
    const { native, emits } = fakeRun();
    new Observe(native).eval({ answer: "yes" });
    expect(emits).toEqual([
      [JSON.stringify({ answer: "yes" }), undefined, undefined, undefined, undefined],
    ]);
  });

  it("awaits one generic-table row as JSON text", async () => {
    const { native, emits } = fakeRun();
    await new Observe(native).record("vala.datasets.events", { value: 1 });
    expect(emits).toEqual([["vala.datasets.events", JSON.stringify({ value: 1 })]]);
  });

  it("throws the projected catalog error an emit was refused with", () => {
    const { native } = fakeRun({
      errorCode: "WYRD_SDK_400_BIFROST_NOT_STARTED",
      errorStatus: 400,
      errorTitle: "Bifrost is not started",
      errorDetail: "call start_bifrost before observing",
    });
    const observe = new Observe(native);
    try {
      observe.drift({ score: 1 });
    } catch (error) {
      expect(error).toBeInstanceOf(WyrdError);
      expect((error as WyrdError).code).toBe("WYRD_SDK_400_BIFROST_NOT_STARTED");
      expect((error as WyrdError).status).toBe(400);
      return;
    }
    throw new Error("expected a WyrdError");
  });

  it("refuses every value JSON.stringify would drop or coerce before native", async () => {
    const cycle: Record<string, unknown> = {};
    cycle.self = cycle;
    const hidden = (): Record<string, unknown> =>
      Object.defineProperty({ visible: 1 }, "hidden", { value: 2, enumerable: false });
    const extra = (): unknown[] => Object.assign([1], { extra: 2 });
    const symbolArray = (): unknown[] => Object.assign([1], { [Symbol("k")]: 2 });
    const refused: readonly [string, unknown][] = [
      ["undefined root", undefined],
      ["function root", () => 1],
      ["nested undefined", { a: undefined }],
      ["nested function", { a: () => 1 }],
      ["nested symbol", { a: Symbol("s") }],
      ["root symbol key", { a: 1, [Symbol("k")]: 2 }],
      ["nested symbol key", { a: { b: 1, [Symbol("k")]: 2 } }],
      ["root non-enumerable property", hidden()],
      ["nested non-enumerable property", { a: hidden() }],
      ["root array non-index property", extra()],
      ["nested array non-index property", { a: extra() }],
      ["root array symbol key", symbolArray()],
      ["nested array symbol key", { a: symbolArray() }],
      ["bigint", { a: 1n }],
      ["NaN", { a: Number.NaN }],
      ["Infinity", { a: Number.POSITIVE_INFINITY }],
      ["unsafe integer", { a: 2 ** 53 }],
      ["array hole", { a: [1, undefined] }],
      ["Map", { a: new Map([["k", 1]]) }],
      ["Date", { a: new Date(0) }],
      ["cycle", cycle],
    ];
    const { native, emits } = fakeRun();
    const observe = new Observe(native);
    for (const [label, value] of refused) {
      const calls = [
        () => observe.drift(value as Record<string, number>),
        () => observe.eval(value),
        () => observe.record("vala.datasets.events", value),
      ];
      for (const emit of calls) {
        let thrown: unknown;
        try {
          await emit();
        } catch (error) {
          thrown = error;
        }
        expect(thrown, label).toBeInstanceOf(WyrdError);
        expect((thrown as WyrdError).code, label).toBe("WYRD_SPEC_400_VALIDATION");
      }
    }
    expect(
      () => observe.eval({ answer: "yes" }, {
        media: [{ id: "page", kind: "document", uri: Symbol("u") as unknown as string }],
      }),
    ).toThrow(WyrdError);
    expect(emits).toEqual([]);
  });

  it("reads every accessor once and sends native exactly the first value", async () => {
    let reads = 0;
    /** A value whose later reads would differ: `first`, then `undefined`. */
    const once = (first: unknown) => ({
      enumerable: true,
      get: () => (reads++ === 0 ? first : undefined),
    });
    const inputs: readonly [string, () => unknown, unknown][] = [
      ["root getter", () => Object.defineProperty({}, "a", once("yes")), { a: "yes" }],
      ["nested getter", () => ({ b: Object.defineProperty({}, "a", once("yes")) }), { b: { a: "yes" } }],
      ["array accessor", () => Object.defineProperty([0], 0, once("yes")), ["yes"]],
      ["nested array accessor", () => ({ b: Object.defineProperty([0], 0, once("yes")) }), { b: ["yes"] }],
      ["unsupported first value", () => ({ b: Object.defineProperty({}, "a", once(1n)) }), undefined],
    ];
    for (const [label, input, expected] of inputs) {
      const { native, emits } = fakeRun();
      const observe = new Observe(native);
      const calls = [
        () => observe.drift(input() as Record<string, number>),
        () => observe.eval(input()),
        () => observe.record("vala.datasets.events", input()),
      ];
      for (const emit of calls) {
        reads = 0;
        let thrown: unknown;
        try {
          await emit();
        } catch (error) {
          thrown = error;
        }
        expect(reads, label).toBe(1);
        if (expected === undefined) {
          expect((thrown as WyrdError).code, label).toBe("WYRD_SPEC_400_VALIDATION");
        } else {
          expect(thrown, label).toBeUndefined();
        }
      }
      const payloads = emits.map((emit) => (emit[0] === "vala.datasets.events" ? emit[1] : emit[0]));
      expect(payloads, label).toEqual(expected === undefined ? [] : Array(3).fill(JSON.stringify(expected)));
    }
  });

  it("refuses media descriptor keys outside the closed shape before native", () => {
    const base = { id: "page", kind: "document", uri: "s3://bucket/page.pdf" };
    const refused: readonly [string, object][] = [
      ["undeclared key", { ...base, extra: 1 }],
      ["symbol key", { ...base, [Symbol("k")]: 1 }],
      ["non-enumerable key", Object.defineProperty({ ...base }, "mediaType", { value: "a/b" })],
    ];
    const { native, emits } = fakeRun();
    const observe = new Observe(native);
    for (const [label, item] of refused) {
      expect(() => observe.eval({ n: 1 }, { media: [item as EvalMediaRef] }), label).toThrow(
        expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }),
      );
    }
    expect(emits).toEqual([]);

    let reads = 0;
    const accessor = Object.defineProperty({ ...base }, "mediaType", {
      enumerable: true,
      get: () => (reads++ === 0 ? "application/pdf" : undefined),
    });
    observe.eval({ n: 1 }, { media: [accessor as EvalMediaRef] });
    expect(reads).toBe(1);
    expect(emits.map((emit) => emit[2])).toEqual([
      JSON.stringify([{ ...base, media_type: "application/pdf" }]),
    ]);
  });

  it("calls native exactly once for valid nested JSON", () => {
    const { native, emits } = fakeRun();
    const context = { a: [1, "x", true, null], b: { c: -2.5 }, d: Number.MAX_SAFE_INTEGER };
    new Observe(native).eval(context);
    expect(emits).toEqual([[JSON.stringify(context), undefined, undefined, undefined, undefined]]);
  });

  it("takes both ids from the active span only when neither is explicit", () => {
    const manager = new StorageContextManager();
    context.setGlobalContextManager(manager);
    try {
      const { native, emits } = fakeRun();
      const observe = new Observe(native);
      observe.eval({ n: 0 });
      withActiveSpan(() => {
        observe.eval({ n: 1 });
        observe.eval(
          { n: 2 },
          { traceId: "0af7651916cd43dd8448eb211c80319c", spanId: "b7ad6b7169203331" },
        );
      });
      context.with(
        trace.setSpan(context.active(), trace.wrapSpanContext({
          traceId: "00000000000000000000000000000000",
          spanId: "0000000000000000",
          traceFlags: TraceFlags.NONE,
        })),
        () => observe.eval({ n: 3 }),
      );
      expect(emits.map((emit) => [emit[3], emit[4]])).toEqual([
        [undefined, undefined],
        [ACTIVE_TRACE, ACTIVE_SPAN],
        ["0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331"],
        [undefined, undefined],
      ]);
    } finally {
      context.disable();
    }
  });
});
