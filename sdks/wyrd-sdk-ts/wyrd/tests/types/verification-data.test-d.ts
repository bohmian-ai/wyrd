// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";
import { z } from "zod";

import type { CardTypes, Observe, VerdictData, VerificationResult, verify } from "wyrd-ai";
import { verdictFormat } from "wyrd-ai";

const Judge = z.object({ score: z.number(), category: z.string() });

// Untyped reads keep the wire result, plain JSON data included.
expectTypeOf<VerificationResult>().toEqualTypeOf<VerificationResult<VerdictData>>();
expectTypeOf<VerificationResult["outcome"]["requirements"][number]["data"]>().toEqualTypeOf<VerdictData | undefined>();
expectTypeOf<CardTypes.VerificationResult>().toExtend<VerificationResult>();

// Typed operations infer the parser's output type.
declare const observe: Observe;
declare const execution: verify.Execution;
expectTypeOf(observe.verifyTyped("judge", { text: "t" }, Judge)).resolves.toEqualTypeOf<
  VerificationResult<z.infer<typeof Judge>>
>();
expectTypeOf(execution.waitTyped({ timeoutMs: 1 }, Judge)).resolves.toEqualTypeOf<
  VerificationResult<z.infer<typeof Judge>>
>();
expectTypeOf(execution.statusTyped(Judge))
  .resolves.toHaveProperty("result")
  .toEqualTypeOf<VerificationResult<z.infer<typeof Judge>> | null | undefined>();
expectTypeOf(
  observe.verifyTyped("judge", { text: "t" }, { parse: (value: unknown) => Number(value) }),
).resolves.toEqualTypeOf<VerificationResult<number>>();

// The authored format is the wire's expanded response format.
expectTypeOf(verdictFormat(Judge)).toExtend<CardTypes.NamedResponseFormat>();
expectTypeOf(verdictFormat()).toExtend<CardTypes.ResponseFormat>();
