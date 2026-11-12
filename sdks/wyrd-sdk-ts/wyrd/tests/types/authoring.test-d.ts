// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type { Cards, verify } from "wyrd-ai";

/** The methods of `C` that take a parameter no value satisfies. */
type Uncallable<C> = {
  [K in keyof C]-?: C[K] extends (...args: infer A) => unknown
    ? { [I in keyof A]-?: [A[I]] extends [never] ? K : never }[number]
    : never;
}[keyof C];

/** Every Card definition class: private constructors hide some from a constructor filter, so they are listed. */
type Classes = Pick<
  typeof verify,
  | "Action"
  | "Agent"
  | "Artifact"
  | "ArtifactFile"
  | "Audit"
  | "AuthoredCard"
  | "Binding"
  | "CardRef"
  | "Check"
  | "CodeSource"
  | "Component"
  | "Data"
  | "Experiment"
  | "InputSlot"
  | "Mcp"
  | "Model"
  | "Operator"
  | "Prompt"
  | "Requirement"
  | "Service"
  | "Step"
  | "Trigger"
  | "Verifier"
>;

// Every setter, add and insert method, and every static factory, accepts some
// value: a field whose wire type is nullable or a union still yields its entry
// type.
expectTypeOf<{ [K in keyof Classes]: Uncallable<Classes[K]["prototype"]> }[keyof Classes]>().toBeNever();
expectTypeOf<{ [K in keyof Classes]: Uncallable<Classes[K]> }[keyof Classes]>().toBeNever();

// Card references at nested canonical positions accept authored Cards.
expectTypeOf<verify.Agent>().toMatchTypeOf<ConstructorParameters<typeof verify.Component>[1]>();
expectTypeOf<verify.Verifier>().toMatchTypeOf<Parameters<verify.Service["addVerifiedBy"]>[0]>();
expectTypeOf<verify.Agent>().toMatchTypeOf<Parameters<typeof verify.Check.agent>[0]>();
expectTypeOf<verify.ArtifactFile>().toMatchTypeOf<Parameters<typeof verify.Action.code>[0]>();
expectTypeOf<verify.Step>().toMatchTypeOf<Parameters<verify.Verifier["addStep"]>[0]>();

// Handle-only values are opaque: a primitive or another family is refused.
expectTypeOf<123>().not.toMatchTypeOf<Parameters<typeof verify.Action.code>[0]>();
expectTypeOf<false>().not.toMatchTypeOf<Parameters<verify.Verifier["addRequirement"]>[0]>();
expectTypeOf<verify.CodeSource>().not.toMatchTypeOf<Parameters<verify.Step["addRequirement"]>[0]>();
expectTypeOf<verify.ArtifactFile>().not.toMatchTypeOf<Parameters<verify.Verifier["addRequirement"]>[0]>();
expectTypeOf<verify.Requirement>().not.toMatchTypeOf<Parameters<typeof verify.Check.code>[0]>();
expectTypeOf<verify.Check>().not.toMatchTypeOf<ConstructorParameters<typeof verify.Step>[1]>();
expectTypeOf<verify.Action>().not.toMatchTypeOf<ConstructorParameters<typeof verify.Requirement>[2]>();
expectTypeOf<verify.CodeSource>().not.toMatchTypeOf<Parameters<verify.Service["addComponent"]>[0]>();

// Each genuine handle, and its canonical object, still fits its position.
expectTypeOf<verify.Requirement>().toMatchTypeOf<Parameters<verify.Step["addRequirement"]>[0]>();
expectTypeOf<verify.CodeSource>().toMatchTypeOf<Parameters<typeof verify.Action.code>[0]>();
expectTypeOf<{ readonly inline: "x" }>().toMatchTypeOf<Parameters<typeof verify.Action.code>[0]>();
expectTypeOf<{
  readonly id: "r";
  readonly statement: "s";
  readonly check: { readonly kind: "assert"; readonly path: "p"; readonly operator: "equals"; readonly expected: 1 };
}>().toMatchTypeOf<Parameters<verify.Verifier["addRequirement"]>[0]>();

// A drift baseline takes an authored Data Card or an exact CardRef.
type DriftSignal = Extract<Parameters<typeof verify.Check.drift>[1]["signal"], { readonly kind: "Distribution" }>;
expectTypeOf<verify.Data>().toMatchTypeOf<DriftSignal["baseline_ref"]>();
expectTypeOf<verify.CardRef>().toMatchTypeOf<DriftSignal["baseline_ref"]>();

// An Agent holds the native Prompt value; a Prompt is not a Card, and a
// Card's version is optional, so its reference may be absent.
expectTypeOf<verify.Prompt>().toMatchTypeOf<NonNullable<verify.AgentOptions["prompt"]>>();
expectTypeOf<verify.Prompt>().not.toMatchTypeOf<verify.AuthoredCard>();
expectTypeOf<ConstructorParameters<typeof verify.Agent>>().toEqualTypeOf<[string, verify.AgentOptions?]>();
expectTypeOf<ReturnType<verify.Service["cardRef"]>>().toEqualTypeOf<verify.CardRef | undefined>();
expectTypeOf<{ readonly type: "verdict" }>().toMatchTypeOf<verify.ResponseFormat>();

// Card definitions are reachable only through `verify`.
// @ts-expect-error the removed `wyrd-ai/authoring` export resolves to nothing
export type Removed = typeof import("wyrd-ai/authoring");

// One declaration name takes a Card file or bundle path and an authored Card.
expectTypeOf<string>().toMatchTypeOf<Parameters<Cards["declare"]>[0]>();
expectTypeOf<verify.Service>().toMatchTypeOf<Parameters<Cards["declare"]>[0]>();
