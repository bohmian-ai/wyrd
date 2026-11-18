// The agent workflow guide's TypeScript program, `examples/refund-agent`, run as a reader runs it: from an empty
// directory against a real server. The program exports its spans through `startTelemetry`, so the trace check reads
// the traces it recorded: the complete reply passes, the reply that skipped its lookup fails, and the reply sent
// without a trace is inconclusive.
import { execFile } from "node:child_process";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

import { expect, vi } from "vitest";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 300_000 });

const EXAMPLE = resolve(import.meta.dirname, "../../../../../examples/refund-agent");

const test = serverTest({ verificationRuntime: true });

test("fails the refund agent's skipped lookup from a reader's directory", async () => {
  const { stdout } = await promisify(execFile)(process.execPath, [join(EXAMPLE, "refund_agent.ts")], {
    cwd: mkdtempSync(join(tmpdir(), "wyrd-ts-refund-reader-")),
    encoding: "utf8",
  });
  expect(stdout).toBe(readFileSync(join(EXAMPLE, "output.txt"), "utf8"));
});
