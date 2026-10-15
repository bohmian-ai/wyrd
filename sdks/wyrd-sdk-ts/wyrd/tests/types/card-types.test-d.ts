// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type {
  CardKind,
  CardRegistrationOutcome,
  CardSummary,
  CardTypes,
  DriftBaselineState,
  Judgment,
  PrincipalSummary,
  RegisteredCard,
  RegisteredVerifierCard,
  TypedCardKind,
} from "wyrd-ai";

// Narrowing a registered envelope by `kind` reaches the Verifier's typed baseline state.
expectTypeOf<
  NonNullable<
    NonNullable<NonNullable<Extract<RegisteredCard, { kind: "Verifier" }>["status"]>["verification"]>["baseline"]
  >["state"]
>().toEqualTypeOf<DriftBaselineState>();

// Deferred kinds keep an untyped spec until the kind is typed.
expectTypeOf<"Workflow">().not.toExtend<TypedCardKind>();
expectTypeOf({
  apiVersion: "wyrd/v1",
  kind: "Workflow",
  metadata: { name: "flow" },
  spec: { steps: [] },
} as const).toExtend<RegisteredCard>();

const undeclared: RegisteredVerifierCard = {
  apiVersion: "wyrd/v1",
  kind: "Verifier",
  metadata: { name: "n" },
  // @ts-expect-error `implementation_kind` is not a VerifierSpec field.
  spec: { implementation_kind: "drift" },
};
void undeclared;

// Registry summaries carry the Rust contract's timestamps and owning enums.
expectTypeOf<CardSummary["created_at"]>().toEqualTypeOf<string>();
expectTypeOf<CardSummary["updated_at"]>().toEqualTypeOf<string>();
expectTypeOf<CardSummary["kind"]>().toEqualTypeOf<CardKind>();
expectTypeOf<CardSummary["status"]>().toEqualTypeOf<CardTypes.CardLifecycleStatus>();

// Response Options Rust always serializes are required and nullable; skipped ones are optional and non-null.
expectTypeOf<CardSummary["artifact_hash"]>().toEqualTypeOf<string | null>();
expectTypeOf<CardRegistrationOutcome["card_blob_uri"]>().toEqualTypeOf<string | null>();
expectTypeOf<PrincipalSummary["email"]>().toEqualTypeOf<string | null>();
expectTypeOf<Judgment["counts"]>().toEqualTypeOf<CardTypes.VerifierCounts | undefined>();
