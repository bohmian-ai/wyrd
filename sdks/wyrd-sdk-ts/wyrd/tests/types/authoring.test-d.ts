// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type { Cards } from "wyrd-ai";
import type * as Authoring from "wyrd-ai/authoring";

/** The methods of `C` that take a parameter no value satisfies. */
type Uncallable<C> = {
  [K in keyof C]-?: C[K] extends (...args: infer A) => unknown
    ? { [I in keyof A]-?: [A[I]] extends [never] ? K : never }[number]
    : never;
}[keyof C];

/** Every authoring class: private constructors hide some from a constructor filter, so they are listed. */
type Classes = Pick<
  typeof Authoring,
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
expectTypeOf<Authoring.Agent>().toMatchTypeOf<ConstructorParameters<typeof Authoring.Component>[1]>();
expectTypeOf<Authoring.Verifier>().toMatchTypeOf<Parameters<Authoring.Service["addVerifiedBy"]>[0]>();
expectTypeOf<Authoring.Agent>().toMatchTypeOf<Parameters<typeof Authoring.Check.agent>[0]>();
expectTypeOf<Authoring.ArtifactFile>().toMatchTypeOf<Parameters<typeof Authoring.Action.code>[0]>();
expectTypeOf<Authoring.Step>().toMatchTypeOf<Parameters<Authoring.Verifier["addStep"]>[0]>();

// Handle-only values are opaque: a primitive or another family is refused.
expectTypeOf<123>().not.toMatchTypeOf<Parameters<typeof Authoring.Action.code>[0]>();
expectTypeOf<false>().not.toMatchTypeOf<Parameters<Authoring.Verifier["addRequirement"]>[0]>();
expectTypeOf<Authoring.CodeSource>().not.toMatchTypeOf<Parameters<Authoring.Step["addRequirement"]>[0]>();
expectTypeOf<Authoring.ArtifactFile>().not.toMatchTypeOf<Parameters<Authoring.Verifier["addRequirement"]>[0]>();
expectTypeOf<Authoring.Requirement>().not.toMatchTypeOf<Parameters<typeof Authoring.Check.code>[0]>();
expectTypeOf<Authoring.Check>().not.toMatchTypeOf<ConstructorParameters<typeof Authoring.Step>[1]>();
expectTypeOf<Authoring.Action>().not.toMatchTypeOf<ConstructorParameters<typeof Authoring.Requirement>[2]>();
expectTypeOf<Authoring.CodeSource>().not.toMatchTypeOf<Parameters<Authoring.Service["addComponent"]>[0]>();

// Each genuine handle, and its canonical object, still fits its position.
expectTypeOf<Authoring.Requirement>().toMatchTypeOf<Parameters<Authoring.Step["addRequirement"]>[0]>();
expectTypeOf<Authoring.CodeSource>().toMatchTypeOf<Parameters<typeof Authoring.Action.code>[0]>();
expectTypeOf<{ readonly inline: "x" }>().toMatchTypeOf<Parameters<typeof Authoring.Action.code>[0]>();
expectTypeOf<{
  readonly id: "r";
  readonly statement: "s";
  readonly check: { readonly kind: "assert"; readonly path: "p"; readonly operator: "equals"; readonly expected: 1 };
}>().toMatchTypeOf<Parameters<Authoring.Verifier["addRequirement"]>[0]>();

// A drift baseline takes an authored Data Card or an exact CardRef.
type DriftSignal = Extract<Parameters<typeof Authoring.Check.drift>[1]["signal"], { readonly kind: "Distribution" }>;
expectTypeOf<Authoring.Data>().toMatchTypeOf<DriftSignal["baseline_ref"]>();
expectTypeOf<Authoring.CardRef>().toMatchTypeOf<DriftSignal["baseline_ref"]>();

// A Prompt's nullable content forms take their canonical null.
expectTypeOf<null>().toMatchTypeOf<Parameters<Authoring.Prompt["messages"]>[0]>();
expectTypeOf<null>().toMatchTypeOf<Parameters<Authoring.Prompt["body"]>[0]>();

// One declaration name takes a Card file or bundle path and an authored Card.
expectTypeOf<string>().toMatchTypeOf<Parameters<Cards["declare"]>[0]>();
expectTypeOf<Authoring.Service>().toMatchTypeOf<Parameters<Cards["declare"]>[0]>();
