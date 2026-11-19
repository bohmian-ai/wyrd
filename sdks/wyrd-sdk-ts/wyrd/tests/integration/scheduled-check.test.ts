// The scheduled-check guide's TypeScript programs, `examples/scheduled-check`, run as a reader runs them. The journey
// declares and pulls the example's `checkout` Service, creates its runtime account, and runs `record.ts` and
// `results.ts` from the reader's directory. The real every-minute scheduler, not the test, judges each window: fast
// requests pass, slow ones fail, and a window with nothing recorded is inconclusive.
import { execFile } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import { promisify } from "node:util";

import { expect, vi } from "vitest";
import { Bifrost, Cards } from "wyrd-ai";
import { z } from "zod";

import { runtimeAccount, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 600_000 });

const EXAMPLE = resolve(import.meta.dirname, "../../../../../examples/scheduled-check");

const test = serverTest({ verificationRuntime: true });

/** Run `program` with `args` from `reader`, as `key` when given, and return what it printed. */
async function run(reader: string, program: string, args: string[] = [], key?: string): Promise<string> {
  const env = key === undefined ? process.env : { ...process.env, WYRD_API_KEY: key };
  const { stdout } = await promisify(execFile)(process.execPath, [join(EXAMPLE, program), ...args], {
    cwd: reader,
    encoding: "utf8",
    env,
  });
  return stdout;
}

/** Wait until at least ten seconds remain in the current minute, so the next samples share one window; return when. */
async function clearOfBoundary(): Promise<number> {
  const second = (Date.now() / 1000) % 60;
  if (second >= 50) await sleep((61 - second) * 1000);
  return Date.now() / 1000;
}

/** The verdict of the scheduled window holding `at`, in epoch seconds, once the real scheduler stores it. */
async function windowVerdict(server: { flushBifrost(): void }, uid: string, at: number): Promise<string> {
  const bifrost = await Bifrost.connect();
  const deadline = Date.now() + 90_000;
  for (;;) {
    server.flushBifrost();
    const rows = await bifrost.sql(
      "SELECT to_unixtime(window_start) AS opened, to_unixtime(window_end) AS closed, verdict" +
        " FROM bifrost.verification.results WHERE verifier_card_uid = $1",
      [uid],
      z.object({ opened: z.coerce.number(), closed: z.coerce.number(), verdict: z.string() }),
    );
    const row = rows.find(({ opened, closed }) => opened <= at && at < closed);
    if (row !== undefined) return row.verdict;
    if (Date.now() >= deadline) throw new Error(`no window holding ${at} was judged`);
    await sleep(1000);
  }
}

/** `text` with every `HH:MM-HH:MM` window, whose times vary per run, replaced. */
function withoutTimes(text: string): string {
  return text.replaceAll(/\d\d:\d\d-\d\d:\d\d/g, "HH:MM-HH:MM");
}

test("judges each scheduled window from a reader's directory", async ({ server }) => {
  const reader = mkdtempSync(join(tmpdir(), "wyrd-ts-scheduled-reader-"));
  cpSync(join(EXAMPLE, "checkout"), join(reader, "checkout"), { recursive: true });
  // What `wyrd declare`, `wyrd pull`, and `wyrd principal credential create-principal --workload-name checkout` do.
  const cards = await Cards.connect();
  const declared = await cards.declareFromPath(join(reader, "checkout/checkout.yaml"));
  await cards.pull(declared.root, join(reader, ".wyrd-bundle"));
  const { key } = await runtimeAccount({ kind: "Service", name: "checkout" });
  const uid = (await cards.get("Verifier/latency-budget@1.0.0")).metadata.uid ?? "";

  const fast = await clearOfBoundary();
  expect(await run(reader, "record.ts", ["80"], key)).toBe("recorded 5 requests at 80 ms\n");
  expect(await windowVerdict(server, uid, fast)).toBe("passed");
  const slow = await clearOfBoundary();
  expect(await run(reader, "record.ts", ["250"], key)).toBe("recorded 5 requests at 250 ms\n");
  expect(await windowVerdict(server, uid, slow)).toBe("failed");
  expect(await windowVerdict(server, uid, Date.now() / 1000)).toBe("inconclusive");

  const expected = readFileSync(join(EXAMPLE, "results-output.txt"), "utf8");
  expect(withoutTimes(await run(reader, "results.ts"))).toBe(withoutTimes(expected));
});
