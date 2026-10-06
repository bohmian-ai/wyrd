import { describe, expect, it } from "vitest";

import type {
  DriftBaselineState,
  RegisteredCard,
  RegisteredVerifierCard,
  TypedCardKind,
} from "@wyrd/sdk";

const BASELINE_DATA = { kind: "Data", name: "training", version: "1.0.0" } as const;

const BUILDING_DRIFT_VERIFIER: RegisteredVerifierCard = {
  apiVersion: "wyrd/v1",
  kind: "Verifier",
  metadata: { name: "churn-drift", space: "retention", version: "1.0.0" },
  spec: {
    implementation: {
      kind: "drift",
      spec: {
        method: "Psi",
        signal: { kind: "Distribution", baseline_ref: BASELINE_DATA, features: ["feature"] },
        condition: { kind: "Statistical" },
        profile: {
          kind: "Psi",
          binning_strategy: { kind: "Quantile", n_bins: 10 },
          threshold: { kind: "Fixed", value: 0.25 },
        },
      },
    },
  },
  status: {
    phase: "active",
    verification: { baseline: { state: "building", data: BASELINE_DATA } },
  },
};

/** Narrow a registered envelope by `kind` and read its typed baseline state. */
function baselineState(card: RegisteredCard): DriftBaselineState | undefined {
  return card.kind === "Verifier" ? card.status?.verification?.baseline?.state : undefined;
}

describe("generated Card envelope types", () => {
  it("narrow a Drift Verifier by kind to its typed baseline state", () => {
    expect(baselineState(BUILDING_DRIFT_VERIFIER)).toBe("building");
  });

  it("type only the supported kinds and keep deferred specs untyped", () => {
    const workflow: RegisteredCard = {
      apiVersion: "wyrd/v1",
      kind: "Workflow",
      metadata: { name: "flow" },
      spec: { steps: [] },
    };
    // @ts-expect-error Workflow specs are not typed until the kind is.
    const deferred: TypedCardKind = "Workflow";
    expect([workflow.kind, deferred]).toEqual(["Workflow", "Workflow"]);
  });

  it("reject a spec field the schema does not declare", () => {
    const card: RegisteredVerifierCard = {
      apiVersion: "wyrd/v1",
      kind: "Verifier",
      metadata: { name: "n" },
      // @ts-expect-error `implementation_kind` is not a VerifierSpec field.
      spec: { implementation_kind: "drift" },
    };
    expect(card.kind).toBe("Verifier");
  });
});
