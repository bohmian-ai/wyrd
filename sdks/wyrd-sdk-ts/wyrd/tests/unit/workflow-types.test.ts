import { describe, expect, it } from "vitest";

import type { JsonValue, Workflow, WorkflowRun } from "@wyrd/sdk";

describe("Workflow types", () => {
  it("accept nested JSON inputs and reject values JSON cannot carry", () => {
    // Only the compiler checks this function; it is never called.
    const run = (workflow: Workflow) => {
      void workflow.run({ code: "diff", limits: { lines: [1, 2] }, strict: true, none: null });
      // @ts-expect-error undefined is dropped by JSON.
      void workflow.run({ code: undefined });
      // @ts-expect-error a function is not JSON.
      void workflow.run({ code: () => "diff" });
      // @ts-expect-error a bigint is not JSON.
      void workflow.run({ count: 1n });
    };
    expect(run).toBeTypeOf("function");
  });

  it("expose every field of the run snapshot without casts", () => {
    const details: JsonValue = { field: "steps[0].inputs.code" };
    const run: WorkflowRun = {
      run_id: "run",
      workflow: null,
      status: "failed",
      outputs: {},
      steps: {
        review: {
          status: "failed",
          text: null,
          structured_output: null,
          attempts: 2,
          started_at: "2026-01-01T00:00:00Z",
          ended_at: "2026-01-01T00:00:01Z",
          error: {
            code: "WYRD_WORKFLOW_504_STEP_TIMEOUT",
            message: "step review timed out",
            details,
            remediation: "Raise the step timeout.",
          },
        },
      },
      created_at: "2026-01-01T00:00:00Z",
      started_at: "2026-01-01T00:00:00Z",
      ended_at: "2026-01-01T00:00:01Z",
      error: {
        code: "WYRD_WORKFLOW_504_STEP_TIMEOUT",
        message: "step review timed out",
        details: {},
        remediation: "Raise the step timeout.",
      },
    };
    const step = run.steps["review"];
    expect(step?.status).toBe("failed");
    expect(step?.attempts).toBe(2);
    expect(step?.error?.details).toEqual({ field: "steps[0].inputs.code" });
    expect(run.error?.remediation).toBe("Raise the step timeout.");
  });
});
