// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type { JsonValue, Workflow, WorkflowRun } from "@wyrd/sdk";

type RunInput = Parameters<Workflow["run"]>[0];

// Nested JSON inputs are accepted; values JSON cannot carry are refused.
expectTypeOf({ code: "diff", limits: { lines: [1, 2] }, strict: true, none: null }).toExtend<RunInput>();
expectTypeOf({ code: undefined }).not.toExtend<RunInput>();
expectTypeOf({ code: () => "diff" }).not.toExtend<RunInput>();
expectTypeOf({ count: 1n }).not.toExtend<RunInput>();

// Every field of the run snapshot is reachable without casts.
type Step = WorkflowRun["steps"][string];
expectTypeOf<Step["attempts"]>().toEqualTypeOf<number>();
expectTypeOf<NonNullable<Step["error"]>["details"]>().toEqualTypeOf<JsonValue>();
expectTypeOf<NonNullable<WorkflowRun["error"]>["remediation"]>().toExtend<string | null | undefined>();
