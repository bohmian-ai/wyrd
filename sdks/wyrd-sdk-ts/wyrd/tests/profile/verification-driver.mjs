// Closed-loop TypeScript caller for the L1 Verification benchmark.
//
// The Rust capacity controller starts this process once, sends one JSON
// configuration line on stdin, and then one command line per measured
// window; each answer is one JSON line on stdout:
//
// - configuration `{server_url, verifier, arrivals: [even, odd],
//   applications: [{bundle, key}]}` answers `{ready: {...}}` once every
//   application's state is loaded;
// - `{run: {clients: N, seconds: S, rate: R}}` runs N bounded Promise loops,
//   each calling its own application's public `observe.verify` for S
//   seconds, and answers `{done: {calls: [...]}}` once every call in flight
//   at the stop has settled. With `rate` null each loop calls back to back;
//   with a rate, loop i's k-th call waits until slot `i + k * N`, due
//   `slot / R` seconds after the window opened, reports the slot's time
//   after the window opened as `due_ns` and how late it started as
//   `lateness_ns`, and sends every slot due before the window ends, however
//   late. The window opens once the recorder snapshot is taken;
// - `{heap: {stem: P}}` saves the V8 sampling heap profile since startup,
//   objects later collected included, as `P.heapprofile`, a V8 heap snapshot
//   of what is still live as `P.heapsnapshot`, and, from a `bench` addon, the
//   addon's sampled native allocations, allocated and still live, as
//   `P.pb.gz`; it answers `{heap: {paths: [...]}}`.
//
// A `bench` build of the addon (`mise run bench:build:sdks`) exports the
// benchmark recorder: the driver installs it at startup, so the shared
// client's and the addon's `sdk.verify` timings record, and every `done` also
// carries `timings`, the recorder snapshots immediately before the window's
// first call and after its last settled; any other build answers
// `timings: null`. The configuration's `profile` asks for diagnostic heap
// captures, which start the V8 sampling heap profiler and the native sampler
// at startup. CPU profiles are external: the controller starts Node with
// `--perf-basic-prof`, so `samply` names JavaScript frames beside native ones.
//
// Each call's clock starts immediately before `verify` and reads twice: when
// it resolves, and when its verdict and every requirement's outcome,
// citations, and data are read from the result. Run and input preparation
// come before the clock; the controller judges the reported results after
// the window. Run from `sdks/wyrd-sdk-ts/wyrd` with
// `node tests/profile/verification-driver.mjs`.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { Session } from "node:inspector/promises";
import { createRequire } from "node:module";
import { writeHeapSnapshot } from "node:v8";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import { WyrdClient, WyrdError, WyrdState } from "wyrd-ai";

/** Nanoseconds since an arbitrary origin, as a Number. */
function now() {
  return Number(process.hrtime.bigint());
}

/** The installed package's directory. */
const PACKAGE = dirname(dirname(fileURLToPath(import.meta.resolve("wyrd-ai"))));

/** The addon file the package loads, or `undefined`. */
const ADDON = readdirSync(PACKAGE).find((name) => name.endsWith(".node"));

/**
 * The addon's benchmark functions, or `null` without a `bench` build. The
 * package loaded the same file, so this is the package's own module.
 */
export const BENCHMARK = (() => {
  if (ADDON === undefined) return null;
  const addon = createRequire(import.meta.url)(join(PACKAGE, ADDON));
  return typeof addon.benchmarkSnapshot === "function" ? addon : null;
})();

/** This process's benchmark recorder snapshot, or `null` without a `bench` addon. */
export function readout() {
  return BENCHMARK === null ? null : JSON.parse(BENCHMARK.benchmarkSnapshot());
}

/** The managed and native heap samplers of a heap-profiled driver. */
export class Heap {
  constructor() {
    this.session = new Session();
  }

  /** Starts sampling every managed allocation, collected ones included, and the addon's native ones. */
  async start() {
    this.session.connect();
    await this.session.post("HeapProfiler.startSampling", {
      includeObjectsCollectedByMajorGC: true,
      includeObjectsCollectedByMinorGC: true,
    });
    BENCHMARK?.benchmarkHeapStart();
  }

  /** Saves the managed allocation profile, the live heap, and the addon's native allocations beside `stem`. */
  async capture(stem) {
    const { profile } = await this.session.post("HeapProfiler.getSamplingProfile");
    const sampled = `${stem}.heapprofile`;
    writeFileSync(sampled, JSON.stringify(profile));
    const paths = [sampled, writeHeapSnapshot(`${stem}.heapsnapshot`)];
    if (BENCHMARK !== null) {
      const native = `${stem}.pb.gz`;
      writeFileSync(native, BENCHMARK.benchmarkHeapProfile());
      paths.push(native);
    }
    return { paths };
  }
}

/** Reads the verdict and each requirement's outcome, citations, and data once. */
export function materialize(result) {
  const { verdict, requirements } = result.outcome;
  for (const requirement of requirements) {
    void requirement.outcome;
    void requirement.citations;
    void requirement.data;
  }
  return { verdict, requirements };
}

/** Awaits `verify()`, then materializes its result, timing both from just before the call. */
export async function timed(verify, clock = now) {
  const sent = clock();
  const result = await verify();
  const returned = clock() - sent;
  const outcome = materialize(result);
  return { result, outcome, returned, ready: clock() - sent };
}

/** Loads every application's pulled state under its own key. */
function load(configuration) {
  return configuration.applications.map(({ bundle, key }) =>
    WyrdState.fromPath(bundle, {
      client: WyrdClient.connect({ serverUrl: configuration.server_url, credential: key }),
    }),
  );
}

/** Runs `clients` callers for `seconds`, closed loop or paced at `rate`: every call, those in flight at the stop included, and the recorder around them. */
async function run(configuration, states, clients, seconds, rate) {
  if (clients < 1 || clients > states.length) {
    throw new Error(`${clients} clients exceed the ${states.length} loaded applications`);
  }
  const before = readout();
  const opened = now();
  const stop = opened + seconds * 1e9;
  let sequences = 0;
  const client = async (state, index) => {
    const calls = [];
    for (let k = 0; ; k++) {
      let due = null;
      if (rate !== null) {
        due = opened + ((index + k * clients) * 1e9) / rate;
        if (due >= stop) break;
        const wait = due - now();
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait / 1e6));
      } else if (now() >= stop) {
        break;
      }
      const sequence = sequences++;
      const observe = state.run("model").observe;
      const arrival = { ...configuration.arrivals[sequence % 2], sequence: String(sequence) };
      const lateness = due === null ? null : Math.max(0, Math.round(now() - due));
      const offset = due === null ? null : Math.round(due - opened);
      const sent = now();
      try {
        const { result, outcome, returned, ready } = await timed(() =>
          observe.verify(configuration.verifier, arrival, { positional: true }),
        );
        calls.push({
          sequence,
          returned_ns: returned,
          ready_ns: ready,
          late: now() >= stop,
          error: null,
          execution_id: result.execution_id,
          outcome,
          lateness_ns: lateness,
          due_ns: offset,
        });
      } catch (error) {
        if (!(error instanceof WyrdError)) throw error;
        const elapsed = now() - sent;
        calls.push({
          sequence,
          returned_ns: elapsed,
          ready_ns: elapsed,
          late: false,
          error: error.code,
          execution_id: null,
          outcome: null,
          lateness_ns: lateness,
          due_ns: offset,
        });
      }
    }
    return calls;
  };
  const settled = await Promise.all(states.slice(0, clients).map(client));
  const after = readout();
  return { calls: settled.flat(), timings: before === null ? null : { before, after } };
}

/** The runtime and native addon this process measures. */
function identity(states) {
  return {
    language: "typescript",
    runtime: `node ${process.versions.node} v8 ${process.versions.v8}`,
    sdk: JSON.parse(readFileSync(join(PACKAGE, "package.json"), "utf8")).version,
    native: ADDON === undefined ? null : join(PACKAGE, ADDON),
    pid: process.pid,
    workers: states.length,
  };
}

/** Writes one JSON answer line to the controller. */
function answer(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

async function main() {
  let configuration;
  let states;
  let heap = null;
  for await (const line of createInterface({ input: process.stdin })) {
    if (!line.trim()) continue;
    const command = JSON.parse(line);
    if (configuration === undefined) {
      configuration = command;
      BENCHMARK?.benchmarkInstall();
      if (configuration.profile?.heap) {
        heap = new Heap();
        await heap.start();
      }
      states = load(configuration);
      answer({ ready: identity(states) });
      continue;
    }
    if (command.heap !== undefined) {
      if (heap === null) throw new Error("the driver is not heap-profiled");
      answer({ heap: await heap.capture(command.heap.stem) });
      continue;
    }
    const { clients, seconds, rate = null } = command.run;
    answer({ done: await run(configuration, states, clients, seconds, rate) });
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await main();
}
