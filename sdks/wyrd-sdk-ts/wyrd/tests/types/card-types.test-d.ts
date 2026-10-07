// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type { DriftBaselineState, RegisteredCard, RegisteredVerifierCard, TypedCardKind } from "@wyrd/sdk";

// Narrowing a registered envelope by `kind` reaches the Verifier's typed baseline state.
expectTypeOf<
  NonNullable<NonNullable<NonNullable<Extract<RegisteredCard, { kind: "Verifier" }>["status"]>["verification"]>["baseline"]>["state"]
>().toEqualTypeOf<DriftBaselineState>();

// Deferred kinds keep an untyped spec until the kind is typed.
expectTypeOf<"Workflow">().not.toExtend<TypedCardKind>();
expectTypeOf({ apiVersion: "wyrd/v1", kind: "Workflow", metadata: { name: "flow" }, spec: { steps: [] } } as const)
  .toExtend<RegisteredCard>();

const undeclared: RegisteredVerifierCard = {
  apiVersion: "wyrd/v1",
  kind: "Verifier",
  metadata: { name: "n" },
  // @ts-expect-error `implementation_kind` is not a VerifierSpec field.
  spec: { implementation_kind: "drift" },
};
void undeclared;
