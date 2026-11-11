// Fixed client-only workloads for macOS Instruments Allocations recordings.
//
// Each workload exercises one operation family offline and checks its
// result, mirroring the Rust and Python workloads:
//
// - `authoring chain8|chain32|shared`: capture and validate a Service chain
//   of 8 or 32 Cards, or 32 parents sharing one child, each rooted at a
//   64 KiB Prompt.
// - `hydration BUNDLE`: hydrate the prepared state bundle at `BUNDLE`.
// - `access typed|agent|run BUNDLE`: hydrate once, then repeat 1,000 typed
//   lookups, Agent/Prompt navigations, or run/Card-view openings.
//
// Run from `sdks/wyrd-sdk-ts/wyrd` with `node tests/profile/client-profile.mjs`.

import assert from "node:assert/strict";

import { verify, WyrdState } from "wyrd-ai";

const { Agent, Component, Service } = verify;
const ACCESS_ITERATIONS = 1_000;

function agent() {
  const payload = "x".repeat(64 * 1024);
  return new Agent("profile-agent", {
    version: "1.0.0",
    prompt: {
      provider: "openai",
      model: "gpt-4o",
      messages: [{ role: "user", content: `${payload} {{report}}` }],
    },
    execution_route: { kind: "wyrd_gateway", deployment: "openai" },
  });
}

function chain(length) {
  let service = new Service("chain-0", { version: "1.0.0", components: [new Component("agent", agent())] });
  for (let index = 1; index < length; index += 1) {
    service = new Service(`chain-${index}`, { version: "1.0.0", components: [new Component("inner", service)] });
  }
  return service;
}

function sharedChild() {
  const child = new Service("child", { version: "1.0.0", components: [new Component("agent", agent())] });
  const parents = Array.from(
    { length: 32 },
    (_, index) =>
      new Component(
        `parent-${index}`,
        new Service(`parent-${index}`, { version: "1.0.0", components: [new Component("child", child)] }),
      ),
  );
  return new Service("parents", { version: "1.0.0", components: parents });
}

function typed(state) {
  for (let index = 0; index < ACCESS_ITERATIONS; index += 1) {
    assert.equal(state.prompt("triage_prompt").kind, "Prompt");
    assert.equal(state.get("triage_prompt").kind, "Prompt");
    state.model("model");
    state.verifier("ok_check");
  }
}

function agentNavigation(state) {
  for (let index = 0; index < ACCESS_ITERATIONS; index += 1) {
    assert.equal(state.agent("agent_triage").prompt().kind, "Prompt");
    assert.equal(state.get("agent_triage").kind, "Agent");
  }
}

function runs(state) {
  for (let index = 0; index < ACCESS_ITERATIONS; index += 1) {
    const run = state.run("agent_triage");
    assert.equal(run.forCard("triage_prompt").runId, run.runId);
  }
}

const [family, operation, bundle] = process.argv.slice(2);
const workloads = {
  "authoring chain8": () => chain(8).validate(),
  "authoring chain32": () => chain(32).validate(),
  "authoring shared": () => sharedChild().validate(),
  "hydration": () => WyrdState.fromPath(operation),
  "access typed": () => typed(WyrdState.fromPath(bundle)),
  "access agent": () => agentNavigation(WyrdState.fromPath(bundle)),
  "access run": () => runs(WyrdState.fromPath(bundle)),
};
const workload = workloads[family === "hydration" ? family : `${family} ${operation}`];
if (workload === undefined) {
  throw new Error("usage: client-profile authoring chain8|chain32|shared | hydration BUNDLE | access typed|agent|run BUNDLE");
}
workload();
