// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";
import { z } from "zod";

import type { CardTypes, Observe, VerdictData, VerdictPredicates, VerificationResult, verify } from "wyrd-ai";
import { verdictFormat } from "wyrd-ai";

const Judge = z.object({ score: z.number(), category: z.string() });

// Untyped reads keep the wire result, plain JSON data included.
expectTypeOf<VerificationResult>().toEqualTypeOf<VerificationResult<VerdictData>>();
expectTypeOf<VerificationResult["outcome"]["requirements"][number]["data"]>().toEqualTypeOf<VerdictData | undefined>();
// The wire result is the SDK result without the verdict predicates the SDK sets.
expectTypeOf<CardTypes.VerificationResult>().toExtend<Omit<VerificationResult, keyof VerdictPredicates>>();
expectTypeOf<VerificationResult["passed"]>().toEqualTypeOf<boolean>();
expectTypeOf<VerificationResult["failed"]>().toEqualTypeOf<boolean>();
expectTypeOf<VerificationResult["inconclusive"]>().toEqualTypeOf<boolean>();

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

// Typed details keep a check's tool calls; each record names its recipient,
// and a historical requirement without calls still reads.
expectTypeOf(execution.detailsTyped(Judge))
  .resolves.toHaveProperty("outcome")
  .toHaveProperty("requirements")
  .items.toHaveProperty("tool_calls")
  .toEqualTypeOf<readonly CardTypes.ToolCallRecord[] | undefined>();
expectTypeOf<CardTypes.StepResult["tool_calls"]>().toEqualTypeOf<readonly CardTypes.ToolCallRecord[] | undefined>();
expectTypeOf<CardTypes.ToolCallRecord["recipient"]>().toEqualTypeOf<"model" | "code">();
expectTypeOf<Omit<CardTypes.RequirementDetails, "tool_calls">>().toExtend<CardTypes.RequirementDetails>();
