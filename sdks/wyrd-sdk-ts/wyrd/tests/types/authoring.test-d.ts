// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

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
  | "CardDefinition"
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
