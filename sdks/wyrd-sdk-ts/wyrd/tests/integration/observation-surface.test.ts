// TypeScript's own observation surface beyond the shared `observe_a_run`
// story: generic records, local emit refusals, and a burst through the byte
// budget. Not mirrored.
import { expect, vi } from "vitest";
import { z } from "zod";

import { Bifrost, TableConfig, WyrdError, WyrdState } from "wyrd-ai";

import { observedServiceTest as test } from "../support/observed-service.js";

vi.setConfig({ testTimeout: 120_000 });

const ANSWERS = "bifrost.datasets.observed_answers";
const SCORES = "bifrost.datasets.observed_scores";
const VALUE_ROW = { type: "object", properties: { value: { type: "integer" } }, required: ["value"] } as const;

const RecordRow = z.object({ value: z.bigint(), card_uid: z.string() });

test("records land in their tables under the view that wrote them", async ({ server, reader, state }) => {
  for (const table of [ANSWERS, SCORES]) {
    const registrar = await Bifrost.connect({ table: TableConfig.fromJsonSchema(table, VALUE_ROW) });
    await registrar.register();
    await registrar.shutdown();
  }
  await state.startBifrost();
  const run = state.run();

  await run.forCard("agent").observe.record(ANSWERS, { value: 41 });
  await run.forCard("model").observe.record(SCORES, { value: 42 });
  await state.shutdown();
  server.flushBifrost();

  const read = (table: string) =>
    reader.sql(`SELECT value, card_uid FROM ${table} WHERE run_id = $1`, [run.runId], RecordRow);
  expect(await read(ANSWERS)).toEqual([{ value: 41n, card_uid: state.cardRef("agent").uid }]);
  expect(await read(SCORES)).toEqual([{ value: 42n, card_uid: state.cardRef("model").uid }]);
});

/** One emit a run refuses, and the catalog code it raises. */
type Refusal = readonly [string, (state: WyrdState, bundle: string) => unknown, string];

test.for<Refusal>([
  [
    "emit before Bifrost starts",
    (_, bundle) => WyrdState.fromPath(bundle).run("model").observe.submit("observed-drift", { latency: 1 }),
    "WYRD_SDK_400_BIFROST_NOT_STARTED",
  ],
  [
    "record into a built-in table",
    (state) => state.run("agent").observe.record("bifrost.drift.observations", { latency: 1 }),
    "WYRD_SDK_400_INVALID_OBSERVATION",
  ],
  [
    "record into an unregistered table",
    (state) => state.run("agent").observe.record("bifrost.datasets.never_registered", { value: 1 }),
    "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
  ],
  ["view of an unknown alias", (state) => state.run().forCard("missing"), "WYRD_SDK_404_UNKNOWN_ALIAS"],
])("refused emit raises its catalog code: %s", async ([, emit, code], { bundle, state }) => {
  await state.startBifrost();

  await expect(Promise.resolve().then(() => emit(state, bundle))).rejects.toMatchObject({ code });

  await state.shutdown();
});

test("submitted burst lands every row exactly once", async ({ server, reader, state }) => {
  await state.startBifrost({ clientByteLimitBytes: 16 * 1024 * 1024 });
  const model = state.run("model");

  for (let observation = 0; observation < 1_000; observation += 1) {
    // Admission is all-or-none per observation: a refused one admitted no
    // row, so resubmitting it after a flush duplicates nothing.
    for (;;) {
      try {
        await model.observe.submit("observed-drift", { latency: observation });
        break;
      } catch (error) {
        if (!(error instanceof WyrdError) || error.code !== "WYRD_CLIENT_429_QUEUE_FULL") {
          throw error;
        }
        await state.flush();
      }
    }
  }
  await state.shutdown();
  server.flushBifrost();

  const perRecord = await reader.sql(
    `SELECT COUNT(*) AS samples FROM bifrost.drift.observations WHERE run_id = $1 GROUP BY record_id`,
    [model.runId],
    z.object({ samples: z.bigint() }),
  );
  expect(perRecord).toHaveLength(1_000);
  expect(new Set(perRecord.map((row) => row.samples))).toEqual(new Set([1n]));
});
