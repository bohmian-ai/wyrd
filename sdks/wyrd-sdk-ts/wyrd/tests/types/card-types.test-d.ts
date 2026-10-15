// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type {
  AgentCard,
  CardKind,
  CardRef,
  CardRegistrationOutcome,
  CardSummary,
  CardTypes,
  DriftBaselineState,
  HydratedCard,
  PrincipalSummary,
  PromptCard,
  RegisteredCard,
  RegisteredModelCard,
  RegisteredVerifierCard,
  TypedCardKind,
  VerificationResult,
  WyrdState,
} from "wyrd-ai";

// Narrowing a registered envelope by `kind` reaches the Verifier's typed baseline state.
expectTypeOf<
  NonNullable<
    NonNullable<NonNullable<Extract<RegisteredCard, { kind: "Verifier" }>["status"]>["verification"]>["baseline"]
  >["state"]
>().toEqualTypeOf<DriftBaselineState>();

// Deferred kinds keep an untyped spec until the kind is typed.
expectTypeOf<"Policy">().not.toExtend<TypedCardKind>();
expectTypeOf({
  apiVersion: "wyrd/v1",
  kind: "Policy",
  metadata: { name: "policy" },
  spec: { rules: [] },
} as const).toExtend<RegisteredCard>();
expectTypeOf<"Workflow">().not.toExtend<CardKind>();

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
expectTypeOf<VerificationResult["run_id"]>().toEqualTypeOf<string | null | undefined>();

// Typed state accessors return hydrated Cards whose Prompt navigation is exact.
declare const state: WyrdState;
expectTypeOf(state.agent("agent")).toEqualTypeOf<AgentCard>();
expectTypeOf(state.agent("agent").prompt()).toEqualTypeOf<PromptCard>();
expectTypeOf(state.agent("agent").prompt().ref).toEqualTypeOf<CardRef>();
expectTypeOf(state.agent("agent").promptRef).toEqualTypeOf<CardRef>();
expectTypeOf(state.prompt("my_prompt").ref).toEqualTypeOf<CardRef>();

// Generic lookup is a kind-discriminated union, narrowed by literal kind only.
const looked = state.get("agent");
expectTypeOf(looked).toEqualTypeOf<HydratedCard>();
if (looked.kind === "Agent") {
  expectTypeOf(looked).toEqualTypeOf<AgentCard>();
  expectTypeOf(looked.prompt().ref).toEqualTypeOf<CardRef>();
} else if (looked.kind === "Prompt") {
  expectTypeOf(looked).toEqualTypeOf<PromptCard>();
} else if (looked.kind === "Model") {
  expectTypeOf(looked).toEqualTypeOf<RegisteredModelCard>();
}
// @ts-expect-error an unnarrowed lookup is not an Agent.
const unnarrowed: AgentCard = state.get("agent");
// @ts-expect-error only an Agent navigates to its Prompt.
state.get("agent").prompt();
// @ts-expect-error a Prompt Card is not a string.
const notString: string = state.agent("agent").prompt();
void unnarrowed;
void notString;
