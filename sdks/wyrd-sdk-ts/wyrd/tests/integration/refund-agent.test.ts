// The agent workflow guide's TypeScript program, `examples/refund-agent`, run as a reader runs it: from an empty
// directory against a real server. The program exports its spans through `startTelemetry`, so the trace check reads
// the traces it recorded: the complete reply passes, the reply that skipped its lookup fails, and the reply sent
// without a trace is inconclusive. The investigation then finds the stored failure and prints its failed requirement and
// the trace it read, and an unknown execution's details are not found.
import { execFile } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

import { expect, vi } from "vitest";
import { verify, WyrdError } from "wyrd-ai";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 300_000 });

const EXAMPLE = resolve(import.meta.dirname, "../../../../../examples/refund-agent");

const test = serverTest({ verificationRuntime: true });

/** Run `program` from `reader` and return what it printed. */
async function run(reader: string, program: string): Promise<string> {
  const { stdout } = await promisify(execFile)(process.execPath, [join(EXAMPLE, program)], {
    cwd: reader,
    encoding: "utf8",
  });
  return stdout;
}

test("fails the refund agent's skipped lookup and investigates it from a reader's directory", async () => {
  const reader = mkdtempSync(join(tmpdir(), "wyrd-ts-refund-reader-"));
  expect(await run(reader, "refund_agent.ts")).toBe(readFileSync(join(EXAMPLE, "output.txt"), "utf8"));
  expect(await run(reader, "investigate.ts")).toBe(readFileSync(join(EXAMPLE, "investigate-output.txt"), "utf8"));

  const unknown = await verify.Execution.connect("01a1ec29-0000-7000-8000-000000000000");
  const refused = await unknown.details().catch((error: unknown) => error);
  expect(refused).toBeInstanceOf(WyrdError);
  expect((refused as WyrdError).code).toBe("WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND");
});
