/**
 * In-memory Card authoring in the Card YAML's own vocabulary, backed by the
 * shared Rust `wyrd_client::authoring` values.
 *
 * Each named Card class, such as {@link Service} or {@link Verifier}, is
 * constructed as `new Service(name, version, options)`; each sub-value, such
 * as {@link Component} or {@link Step}, with the fields that identify it.
 * Every other method is named for the YAML field it edits, in camelCase, and
 * takes that field's YAML value: options and nested objects keep the YAML's
 * snake_case keys, references stay strings such as `inputs.report.text`, and
 * operators stay `"equals"` or their object form. A setter such as
 * {@link Service.verifiedBy} replaces the whole field; an add method such as
 * {@link Service.addVerifiedBy} appends one entry. Both mutate the receiver and
 * return it.
 *
 * Construction and editing never validate, read files, or touch the network,
 * so an incomplete definition or one with a forward reference can be
 * assembled freely. A Card or exact {@link CardRef} may stand at any reference
 * position, nested in plain objects included; passing a child copies its
 * current value, so later edits to the child change nothing already passed.
 * {@link AuthoredCard.validate} runs the shared Rust validation, and
 * `Cards.declare` and `WyrdState.declare` register the graph, publishing each
 * local Artifact first.
 *
 * @module
 */

import { createRequire } from "node:module";

import type { NativeAuthored as NativeAuthoredHandle, NativeValue as NativeValueHandle } from "../index.cjs";
import type * as Wire from "./card-types.js";
import { authoredNative, authoredOf, holdAuthored, jsonSnapshot, lifecycleValue } from "./native.js";

const require = createRequire(import.meta.url);
const { NativeAuthored, NativeValue } = require("../index.cjs") as typeof import("../index.cjs");

/**
 * `T` as authored: wherever the YAML takes an exact Card reference, an
 * {@link AuthoredCard} or {@link CardRef} too; wherever it takes a code source
 * or Artifact selection, a {@link CodeSource} or {@link ArtifactFile} too; and
 * wherever it takes a canonical sub-value, that value's authoring class too.
 */
export type Authorable<T> = [Wire.JsonValue] extends [T]
  ? T
  : T extends Wire.CardRef
    ? T | AuthoredCard | CardRef
    : T extends Wire.CodeSourceInline | Wire.CodeSourceArtifact
      ? Shape<T> | CodeSource | ArtifactFile
      : T extends Wire.WorkspaceSourceArtifact
        ? Shape<T> | ArtifactFile
        : T extends Wire.Step
          ? Shape<T> | Step
          : T extends Wire.Requirement
            ? Shape<T> | Requirement
            : T extends Wire.Action
              ? Shape<T> | Action
              : T extends Wire.Check
                ? Shape<T> | Check
                : T extends Wire.ServiceComponent
                  ? Shape<T> | Component
                  : T extends Wire.VerificationBinding
                    ? Shape<T> | Binding
                    : T extends Wire.TypedInputSlot
                      ? Shape<T> | InputSlot
                      : Shape<T>;

/** The members of `T`, each {@link Authorable}. */
type Shape<T> = T extends readonly (infer U)[]
  ? readonly Authorable<U>[]
  : T extends object
    ? { readonly [K in keyof T]: Authorable<T[K]> }
    : T;

/** `S`'s fields as authored, each optional. */
type Fields<S> = { readonly [K in keyof S]?: Authorable<S[K]> };

/** The value the field `K` of `S` takes. */
type Value<S, K extends keyof S> = Exclude<S[K], undefined>;

/** One entry of the list field `K` of `S`. */
type Item<S, K extends keyof S> = NonNullable<S[K]> extends readonly (infer U)[] ? U : never;

/** One entry's value of the map field `K` of `S`. */
type Entry<S, K extends keyof S> = NonNullable<S[K]> extends { readonly [key: string]: infer V } ? V : never;

/** `Omit` applied to each member of the union `T`. */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

/**
 * One verification binding: a {@link Binding} or its canonical object, or a
 * {@link Verifier}, exact {@link CardRef} or reference path bound with no
 * trigger or Operators.
 */
export type BindingArg = Authorable<Wire.VerificationBinding> | Verifier | CardRef | string;

/**
 * One Verifier input slot: an {@link InputSlot}, a canonical slot object, a
 * source slot's reference string, or the bare JSON Schema of a `json` slot.
 * Values are schemas, never examples to infer one from.
 */
export type InputSlotArg = InputSlot | Wire.InputSlot | Readonly<Record<string, Wire.JsonValue>> | boolean;

/** Service spec fields as authored. */
type ServiceFields = Omit<Fields<Wire.ServiceSpec>, "verified_by"> & { readonly verified_by?: readonly BindingArg[] };
/** Agent spec fields as authored. */
type AgentFields = Omit<Fields<Wire.AgentSpec>, "verified_by"> & { readonly verified_by?: readonly BindingArg[] };
/** Prompt spec fields as authored. */
type PromptFields = Fields<Wire.PromptSpec>;
/** Verifier spec fields as authored. */
type VerifierFields = Omit<Fields<Wire.VerifierSpec>, "inputs"> & {
  readonly inputs?: Readonly<Record<string, InputSlotArg>>;
};
/** Artifact spec fields as authored. */
type ArtifactFields = Fields<Wire.ArtifactSpec>;
/** Model spec fields as authored. */
type ModelFields = Fields<Wire.ModelSpec>;
/** Data spec fields as authored. */
type DataFields = Fields<Wire.DataSpec>;
/** Experiment spec fields as authored. */
type ExperimentFields = Fields<Wire.ExperimentSpec>;
/** Mcp spec fields as authored. */
type McpFields = Fields<Wire.McpSpec>;
/** Audit spec fields as authored. */
type AuditFields = Fields<Wire.AuditSpec>;
/** Trigger spec fields as authored: the activation keys of either form, flat. */
type TriggerFields = Fields<
  Pick<Wire.TriggerSpec, "description" | "kind"> &
    Pick<Wire.TriggerSpecSchedule, "cron" | "tz"> &
    Pick<Wire.TriggerSpecGroupEnd, "after">
>;
/** Operator spec fields as authored: the flattened `action` object apart. */
type OperatorFields = Fields<Pick<Wire.OperatorSpec, "description" | "budget">> & {
  readonly action?: DistributiveOmit<Wire.OperatorSpec, "description" | "budget">;
};
/** Service component fields as authored. */
type ComponentFields = Omit<Fields<Wire.ServiceComponent>, "verified_by"> & {
  readonly verified_by?: readonly BindingArg[];
};
/** Verification binding fields as authored. */
type BindingFields = Fields<Wire.VerificationBinding>;
/** Step fields as authored. */
type StepFields = Fields<Wire.Step>;
/** Input slot fields as authored. */
type InputSlotFields = Fields<Pick<Wire.TypedInputSlotMedia, "max_bytes">>;
/** Action fields as authored. */
type ActionFields = Fields<Pick<Wire.ActionAgent, "inputs" | "workspace"> & Pick<Wire.ActionCode, "runtime">>;
/** Check fields as authored. */
type CheckFields = Fields<
  Pick<Wire.CheckAgent, "inputs" | "workspace"> & Pick<Wire.CheckCode, "runtime" | "response_format">
>;

/** {@link Service} spec options. */
export type ServiceOptions = ServiceFields;
/** {@link Agent} spec options. */
export type AgentOptions = AgentFields;
/** {@link Prompt} spec options. */
export type PromptOptions = PromptFields;
/** {@link Verifier} spec options. */
export type VerifierOptions = VerifierFields;
/** {@link Artifact} spec options. */
export type ArtifactOptions = ArtifactFields;
/** {@link Model} spec options. */
export type ModelOptions = ModelFields;
/** {@link Data} spec options. */
export type DataOptions = DataFields;
/** {@link Experiment} spec options. */
export type ExperimentOptions = ExperimentFields;
/** {@link Mcp} spec options. */
export type McpOptions = McpFields;
/** {@link Audit} spec options. */
export type AuditOptions = AuditFields;
/** {@link Trigger} spec options. */
export type TriggerOptions = TriggerFields;
/** {@link Operator} spec options. */
export type OperatorOptions = OperatorFields;
/** {@link Component} fields other than its alias and reference. */
export type ComponentOptions = Omit<ComponentFields, "alias" | "ref">;
/** {@link Binding} fields other than its Verifier. */
export type BindingOptions = Omit<BindingFields, "verifier">;
/** {@link Step} fields other than its ID and action. */
export type StepOptions = Omit<StepFields, "id" | "action">;
/** {@link InputSlot.media} fields other than its formats. */
export type MediaSlotOptions = InputSlotFields;
/** {@link Action.agent} fields other than its Agent. */
export type AgentActionOptions = Omit<Fields<Wire.ActionAgent>, "kind" | "agent">;
/** {@link Action.code} fields other than its source and entrypoint. */
export type CodeActionOptions = Omit<Fields<Wire.ActionCode>, "kind" | "source" | "entrypoint">;
/** {@link Check.agent} fields other than its Agent. */
export type AgentCheckOptions = Omit<Fields<Wire.CheckAgent>, "kind" | "agent">;
/** {@link Check.code} fields other than its source and entrypoint. */
export type CodeCheckOptions = Omit<Fields<Wire.CheckCode>, "kind" | "source" | "entrypoint">;

/**
 * `input` as one native argument: its JSON snapshot, with each authoring
 * value inside copied at its position.
 *
 * @throws a `WYRD_SPEC_400_VALIDATION` `WyrdError` naming `field` for a value
 * JSON cannot carry exactly.
 */
function argument(field: string, input: unknown): NativeValueHandle {
  const snapshot = jsonSnapshot(field, input, authoredNative);
  return NativeValue.compose(
    snapshot.value,
    snapshot.holes.map(({ path }) => [...path]),
    snapshot.holes.map(({ hole }) => hole),
  );
}

/** A new authoring value of `prototype`'s class over `native`. */
function adopt<T extends object>(prototype: T, native: NativeAuthoredHandle): T {
  const value = Object.create(prototype) as T;
  holdAuthored(value, native);
  return value;
}

/** Set the field `field` of `owner` from `input` and return `owner`. */
function set<T extends object>(owner: T, field: string, input: unknown): T {
  authoredOf(owner).set(field, argument(field, input));
  return owner;
}

/** Append `input` to the list `field` of `owner` and return `owner`. */
function add<T extends object>(owner: T, field: string, input: unknown): T {
  authoredOf(owner).add(field, argument(field, input));
  return owner;
}

/** Insert the entry `key` into the map `field` of `owner` and return `owner`. */
function insert<T extends object>(owner: T, field: string, key: string, input: unknown): T {
  authoredOf(owner).insert(field, key, argument(field, input));
  return owner;
}

/** Set every defined field of `options` on `owner`, in key order. */
function apply(owner: object, options: object): void {
  for (const [field, input] of Object.entries(options)) {
    if (input !== undefined) {
      set(owner, field, input);
    }
  }
}

/**
 * A named Card under construction: its labels and annotations, its exact
 * reference, its spec as authored, and validation of the graph it roots.
 */
export abstract class AuthoredCard {
  /**
   * Hold `native` as this Card's shared Rust value.
   *
   * @param native - The native Card.
   */
  protected constructor(native: NativeAuthoredHandle) {
    holdAuthored(this, native);
  }

  /**
   * Set the whole `metadata.labels` map.
   *
   * @param labels - The new labels.
   * @returns This Card.
   */
  labels(labels: Readonly<Record<string, string>>): this {
    return set(this, "labels", labels);
  }

  /**
   * Insert the label `key`; a repeated key is refused at validation.
   *
   * @param key - The label key.
   * @param value - The label value.
   * @returns This Card.
   */
  addLabel(key: string, value: string): this {
    return insert(this, "labels", key, value);
  }

  /**
   * Set the whole `metadata.annotations` map.
   *
   * @param annotations - The new annotations.
   * @returns This Card.
   */
  annotations(annotations: Readonly<Record<string, string>>): this {
    return set(this, "annotations", annotations);
  }

  /**
   * Insert the annotation `key`; a repeated key is refused at validation.
   *
   * @param key - The annotation key.
   * @param value - The annotation value.
   * @returns This Card.
   */
  addAnnotation(key: string, value: string): this {
    return insert(this, "annotations", key, value);
  }

  /**
   * The exact reference this Card registers as.
   *
   * @returns The reference.
   */
  cardRef(): CardRef {
    return adopt(CardRef.prototype, authoredOf(this).cardRef());
  }

  /**
   * The spec as authored so far, before validation; a {@link CodeSource.file}
   * position is `null` until its graph is validated.
   *
   * @returns The authored spec JSON.
   */
  spec(): Wire.JsonValue {
    return authoredOf(this).spec() as Wire.JsonValue;
  }

  /**
   * Validate the complete graph this Card roots exactly as its declaration
   * would, reading each local code file, without registering or uploading
   * anything.
   *
   * @throws `WyrdError` from the shared validation, such as
   * `WYRD_LOADER_400_INVALID_ENVELOPE` naming each refused YAML field path.
   */
  validate(): void {
    lifecycleValue<null>(authoredOf(this).validate());
  }
}

/** A Service Card: a running system, its components, and the Verifiers that check it. */
export class Service extends AuthoredCard {
  /**
   * An empty Service Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: ServiceOptions = {}) {
    super(NativeAuthored.card("Service", name, version));
    apply(this, options);
  }

  /**
   * Set `description`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `description`.
   * @returns This value.
   */
  description(value: Value<ServiceFields, "description">): this {
    return set(this, "description", value);
  }

  /**
   * Set `service_type`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `service_type`.
   * @returns This value.
   */
  serviceType(value: Value<ServiceFields, "service_type">): this {
    return set(this, "service_type", value);
  }

  /**
   * Set `entry_point`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `entry_point`.
   * @returns This value.
   */
  entryPoint(value: Value<ServiceFields, "entry_point">): this {
    return set(this, "entry_point", value);
  }

  /**
   * Set `runtime`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `runtime`.
   * @returns This value.
   */
  runtime(value: Value<ServiceFields, "runtime">): this {
    return set(this, "runtime", value);
  }

  /**
   * Set `content_hash`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `content_hash`.
   * @returns This value.
   */
  contentHash(value: Value<ServiceFields, "content_hash">): this {
    return set(this, "content_hash", value);
  }

  /**
   * Set `lock_hash`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `lock_hash`.
   * @returns This value.
   */
  lockHash(value: Value<ServiceFields, "lock_hash">): this {
    return set(this, "lock_hash", value);
  }

  /**
   * Set the whole `components` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  components(items: Value<ServiceFields, "components">): this {
    return set(this, "components", items);
  }

  /**
   * Append one entry to `components`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addComponent(item: Item<ServiceFields, "components">): this {
    return add(this, "components", item);
  }

  /**
   * Set the whole `verified_by` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  verifiedBy(items: Value<ServiceFields, "verified_by">): this {
    return set(this, "verified_by", items);
  }

  /**
   * Append one entry to `verified_by`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addVerifiedBy(item: Item<ServiceFields, "verified_by">): this {
    return add(this, "verified_by", item);
  }

  /**
   * Set the whole `tables` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  tables(items: Value<ServiceFields, "tables">): this {
    return set(this, "tables", items);
  }

  /**
   * Append one entry to `tables`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addTable(item: Item<ServiceFields, "tables">): this {
    return add(this, "tables", item);
  }

  /**
   * Set the whole `credential_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  credentialRefs(items: Value<ServiceFields, "credential_refs">): this {
    return set(this, "credential_refs", items);
  }

  /**
   * Append one entry to `credential_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addCredentialRef(item: Item<ServiceFields, "credential_refs">): this {
    return add(this, "credential_refs", item);
  }

  /**
   * Set the whole `deployment` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  deployment(entries: Value<ServiceFields, "deployment">): this {
    return set(this, "deployment", entries);
  }

  /**
   * Insert the entry `key` into `deployment`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addDeployment(key: string, value: Entry<ServiceFields, "deployment">): this {
    return insert(this, "deployment", key, value);
  }

  /**
   * Set the whole `service_config` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  serviceConfig(entries: Value<ServiceFields, "service_config">): this {
    return set(this, "service_config", entries);
  }

  /**
   * Insert the entry `key` into `service_config`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addServiceConfig(key: string, value: Entry<ServiceFields, "service_config">): this {
    return insert(this, "service_config", key, value);
  }

  /**
   * Set the whole `metadata` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  metadata(entries: Value<ServiceFields, "metadata">): this {
    return set(this, "metadata", entries);
  }

  /**
   * Insert the entry `key` into `metadata`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addMetadata(key: string, value: Entry<ServiceFields, "metadata">): this {
    return insert(this, "metadata", key, value);
  }
}

/** An Agent Card: a prompt run through an execution route with tools. */
export class Agent extends AuthoredCard {
  /**
   * An empty Agent Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: AgentOptions = {}) {
    super(NativeAuthored.card("Agent", name, version));
    apply(this, options);
  }

  /**
   * Set `prompt`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `prompt`.
   * @returns This value.
   */
  prompt(value: Value<AgentFields, "prompt">): this {
    return set(this, "prompt", value);
  }

  /**
   * Set `execution_route`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `execution_route`.
   * @returns This value.
   */
  executionRoute(value: Value<AgentFields, "execution_route">): this {
    return set(this, "execution_route", value);
  }

  /**
   * Set `run_config`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `run_config`.
   * @returns This value.
   */
  runConfig(value: Value<AgentFields, "run_config">): this {
    return set(this, "run_config", value);
  }

  /**
   * Set the whole `tool_names` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  toolNames(items: Value<AgentFields, "tool_names">): this {
    return set(this, "tool_names", items);
  }

  /**
   * Append one entry to `tool_names`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addToolName(item: Item<AgentFields, "tool_names">): this {
    return add(this, "tool_names", item);
  }

  /**
   * Set the whole `verified_by` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  verifiedBy(items: Value<AgentFields, "verified_by">): this {
    return set(this, "verified_by", items);
  }

  /**
   * Append one entry to `verified_by`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addVerifiedBy(item: Item<AgentFields, "verified_by">): this {
    return add(this, "verified_by", item);
  }
}

/** A Prompt Card: provider, model, and the authored prompt. */
export class Prompt extends AuthoredCard {
  /**
   * An empty Prompt Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: PromptOptions = {}) {
    super(NativeAuthored.card("Prompt", name, version));
    apply(this, options);
  }

  /**
   * Set `provider`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `provider`.
   * @returns This value.
   */
  provider(value: Value<PromptFields, "provider">): this {
    return set(this, "provider", value);
  }

  /**
   * Set `model`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `model`.
   * @returns This value.
   */
  model(value: Value<PromptFields, "model">): this {
    return set(this, "model", value);
  }

  /**
   * Set `system`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `system`.
   * @returns This value.
   */
  system(value: Value<PromptFields, "system">): this {
    return set(this, "system", value);
  }

  /**
   * Set `settings`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `settings`.
   * @returns This value.
   */
  settings(value: Value<PromptFields, "settings">): this {
    return set(this, "settings", value);
  }

  /**
   * Set `response_format`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `response_format`.
   * @returns This value.
   */
  responseFormat(value: Value<PromptFields, "response_format">): this {
    return set(this, "response_format", value);
  }

  /**
   * Set `format`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `format`.
   * @returns This value.
   */
  format(value: Value<PromptFields, "format">): this {
    return set(this, "format", value);
  }

  /**
   * Set the whole `messages` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  messages(items: Value<PromptFields, "messages">): this {
    return set(this, "messages", items);
  }

  /**
   * Append one entry to `messages`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addMessage(item: Item<PromptFields, "messages">): this {
    return add(this, "messages", item);
  }

  /**
   * Set the whole `body` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  body(entries: Value<PromptFields, "body">): this {
    return set(this, "body", entries);
  }

  /**
   * Insert the entry `key` into `body`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addBody(key: string, value: Entry<PromptFields, "body">): this {
    return insert(this, "body", key, value);
  }
}

/** A Model Card: a trained model's interface, task, and signature. */
export class Model extends AuthoredCard {
  /**
   * An empty Model Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: ModelOptions = {}) {
    super(NativeAuthored.card("Model", name, version));
    apply(this, options);
  }

  /**
   * Set `interface`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `interface`.
   * @returns This value.
   */
  interface(value: Value<ModelFields, "interface">): this {
    return set(this, "interface", value);
  }

  /**
   * Set `task_type`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `task_type`.
   * @returns This value.
   */
  taskType(value: Value<ModelFields, "task_type">): this {
    return set(this, "task_type", value);
  }

  /**
   * Set `signature`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `signature`.
   * @returns This value.
   */
  signature(value: Value<ModelFields, "signature">): this {
    return set(this, "signature", value);
  }

  /**
   * Set `sample_input`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `sample_input`.
   * @returns This value.
   */
  sampleInput(value: Value<ModelFields, "sample_input">): this {
    return set(this, "sample_input", value);
  }

  /**
   * Set the whole `card_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  cardRefs(items: Value<ModelFields, "card_refs">): this {
    return set(this, "card_refs", items);
  }

  /**
   * Append one entry to `card_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addCardRef(item: Item<ModelFields, "card_refs">): this {
    return add(this, "card_refs", item);
  }
}

/** A Data Card: a dataset's interface, schema, splits, and target columns. */
export class Data extends AuthoredCard {
  /**
   * An empty Data Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: DataOptions = {}) {
    super(NativeAuthored.card("Data", name, version));
    apply(this, options);
  }

  /**
   * Set `interface`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `interface`.
   * @returns This value.
   */
  interface(value: Value<DataFields, "interface">): this {
    return set(this, "interface", value);
  }

  /**
   * Set `schema`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `schema`.
   * @returns This value.
   */
  schema(value: Value<DataFields, "schema">): this {
    return set(this, "schema", value);
  }

  /**
   * Set `sql`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `sql`.
   * @returns This value.
   */
  sql(value: Value<DataFields, "sql">): this {
    return set(this, "sql", value);
  }

  /**
   * Set `stats`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `stats`.
   * @returns This value.
   */
  stats(value: Value<DataFields, "stats">): this {
    return set(this, "stats", value);
  }

  /**
   * Set the whole `card_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  cardRefs(items: Value<DataFields, "card_refs">): this {
    return set(this, "card_refs", items);
  }

  /**
   * Append one entry to `card_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addCardRef(item: Item<DataFields, "card_refs">): this {
    return add(this, "card_refs", item);
  }

  /**
   * Set the whole `target_columns` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  targetColumns(items: Value<DataFields, "target_columns">): this {
    return set(this, "target_columns", items);
  }

  /**
   * Append one entry to `target_columns`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addTargetColumn(item: Item<DataFields, "target_columns">): this {
    return add(this, "target_columns", item);
  }

  /**
   * Set the whole `splits` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  splits(entries: Value<DataFields, "splits">): this {
    return set(this, "splits", entries);
  }

  /**
   * Insert the entry `key` into `splits`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addSplit(key: string, value: Entry<DataFields, "splits">): this {
    return insert(this, "splits", key, value);
  }
}

/** An Experiment Card: the targets and parameters of a set of runs, with their summary metrics and best run. */
export class Experiment extends AuthoredCard {
  /**
   * An empty Experiment Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: ExperimentOptions = {}) {
    super(NativeAuthored.card("Experiment", name, version));
    apply(this, options);
  }

  /**
   * Set `experiment_type`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `experiment_type`.
   * @returns This value.
   */
  experimentType(value: Value<ExperimentFields, "experiment_type">): this {
    return set(this, "experiment_type", value);
  }

  /**
   * Set `description`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `description`.
   * @returns This value.
   */
  description(value: Value<ExperimentFields, "description">): this {
    return set(this, "description", value);
  }

  /**
   * Set `best_run_ref`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `best_run_ref`.
   * @returns This value.
   */
  bestRunRef(value: Value<ExperimentFields, "best_run_ref">): this {
    return set(this, "best_run_ref", value);
  }

  /**
   * Set the whole `target_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  targetRefs(items: Value<ExperimentFields, "target_refs">): this {
    return set(this, "target_refs", items);
  }

  /**
   * Append one entry to `target_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addTargetRef(item: Item<ExperimentFields, "target_refs">): this {
    return add(this, "target_refs", item);
  }

  /**
   * Set the whole `run_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  runRefs(items: Value<ExperimentFields, "run_refs">): this {
    return set(this, "run_refs", items);
  }

  /**
   * Append one entry to `run_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addRunRef(item: Item<ExperimentFields, "run_refs">): this {
    return add(this, "run_refs", item);
  }

  /**
   * Set the whole `summary_metrics` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  summaryMetrics(items: Value<ExperimentFields, "summary_metrics">): this {
    return set(this, "summary_metrics", items);
  }

  /**
   * Append one entry to `summary_metrics`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addSummaryMetric(item: Item<ExperimentFields, "summary_metrics">): this {
    return add(this, "summary_metrics", item);
  }

  /**
   * Set the whole `card_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  cardRefs(items: Value<ExperimentFields, "card_refs">): this {
    return set(this, "card_refs", items);
  }

  /**
   * Append one entry to `card_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addCardRef(item: Item<ExperimentFields, "card_refs">): this {
    return add(this, "card_refs", item);
  }

  /**
   * Set the whole `default_parameters` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  defaultParameters(entries: Value<ExperimentFields, "default_parameters">): this {
    return set(this, "default_parameters", entries);
  }

  /**
   * Insert the entry `key` into `default_parameters`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addDefaultParameter(key: string, value: Entry<ExperimentFields, "default_parameters">): this {
    return insert(this, "default_parameters", key, value);
  }

  /**
   * Set the whole `details` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  details(entries: Value<ExperimentFields, "details">): this {
    return set(this, "details", entries);
  }

  /**
   * Insert the entry `key` into `details`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addDetail(key: string, value: Entry<ExperimentFields, "details">): this {
    return insert(this, "details", key, value);
  }
}

/** An Mcp Card: an MCP server and the tools it exposes. */
export class Mcp extends AuthoredCard {
  /**
   * An empty Mcp Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: McpOptions = {}) {
    super(NativeAuthored.card("Mcp", name, version));
    apply(this, options);
  }

  /**
   * Set `description`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `description`.
   * @returns This value.
   */
  description(value: Value<McpFields, "description">): this {
    return set(this, "description", value);
  }

  /**
   * Set `server_name`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `server_name`.
   * @returns This value.
   */
  serverName(value: Value<McpFields, "server_name">): this {
    return set(this, "server_name", value);
  }

  /**
   * Set `transport`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `transport`.
   * @returns This value.
   */
  transport(value: Value<McpFields, "transport">): this {
    return set(this, "transport", value);
  }

  /**
   * Set the whole `tool_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  toolRefs(items: Value<McpFields, "tool_refs">): this {
    return set(this, "tool_refs", items);
  }

  /**
   * Append one entry to `tool_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addToolRef(item: Item<McpFields, "tool_refs">): this {
    return add(this, "tool_refs", item);
  }

  /**
   * Set the whole `scopes` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  scopes(items: Value<McpFields, "scopes">): this {
    return set(this, "scopes", items);
  }

  /**
   * Append one entry to `scopes`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addScope(item: Item<McpFields, "scopes">): this {
    return add(this, "scopes", item);
  }

  /**
   * Set the whole `details` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  details(entries: Value<McpFields, "details">): this {
    return set(this, "details", entries);
  }

  /**
   * Insert the entry `key` into `details`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addDetail(key: string, value: Entry<McpFields, "details">): this {
    return insert(this, "details", key, value);
  }
}

/** An Audit Card: subjects, policies, and evidence. */
export class Audit extends AuthoredCard {
  /**
   * An empty Audit Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: AuditOptions = {}) {
    super(NativeAuthored.card("Audit", name, version));
    apply(this, options);
  }

  /**
   * Set `description`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `description`.
   * @returns This value.
   */
  description(value: Value<AuditFields, "description">): this {
    return set(this, "description", value);
  }

  /**
   * Set the whole `subject_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  subjectRefs(items: Value<AuditFields, "subject_refs">): this {
    return set(this, "subject_refs", items);
  }

  /**
   * Append one entry to `subject_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addSubjectRef(item: Item<AuditFields, "subject_refs">): this {
    return add(this, "subject_refs", item);
  }

  /**
   * Set the whole `policy_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  policyRefs(items: Value<AuditFields, "policy_refs">): this {
    return set(this, "policy_refs", items);
  }

  /**
   * Append one entry to `policy_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addPolicyRef(item: Item<AuditFields, "policy_refs">): this {
    return add(this, "policy_refs", item);
  }

  /**
   * Set the whole `evidence_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  evidenceRefs(items: Value<AuditFields, "evidence_refs">): this {
    return set(this, "evidence_refs", items);
  }

  /**
   * Append one entry to `evidence_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addEvidenceRef(item: Item<AuditFields, "evidence_refs">): this {
    return add(this, "evidence_refs", item);
  }

  /**
   * Set the whole `details` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  details(entries: Value<AuditFields, "details">): this {
    return set(this, "details", entries);
  }

  /**
   * Insert the entry `key` into `details`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addDetail(key: string, value: Entry<AuditFields, "details">): this {
    return insert(this, "details", key, value);
  }
}

/** A Trigger Card: when a binding's Verifier runs. */
export class Trigger extends AuthoredCard {
  /**
   * An empty Trigger Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: TriggerOptions = {}) {
    super(NativeAuthored.card("Trigger", name, version));
    apply(this, options);
  }

  /**
   * Set `description`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `description`.
   * @returns This value.
   */
  description(value: Value<TriggerFields, "description">): this {
    return set(this, "description", value);
  }

  /**
   * Set `kind`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `kind`.
   * @returns This value.
   */
  kind(value: Value<TriggerFields, "kind">): this {
    return set(this, "kind", value);
  }

  /**
   * Set `cron`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `cron`.
   * @returns This value.
   */
  cron(value: Value<TriggerFields, "cron">): this {
    return set(this, "cron", value);
  }

  /**
   * Set `tz`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `tz`.
   * @returns This value.
   */
  tz(value: Value<TriggerFields, "tz">): this {
    return set(this, "tz", value);
  }

  /**
   * Set `after`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `after`.
   * @returns This value.
   */
  after(value: Value<TriggerFields, "after">): this {
    return set(this, "after", value);
  }
}

/** An Operator Card: what runs when a binding's Verifier fails or changes verdict. */
export class Operator extends AuthoredCard {
  /**
   * An empty Operator Card named `name` at `version`, with any spec fields given
   * in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: OperatorOptions = {}) {
    super(NativeAuthored.card("Operator", name, version));
    apply(this, options);
  }

  /**
   * Set `description`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `description`.
   * @returns This value.
   */
  description(value: Value<OperatorFields, "description">): this {
    return set(this, "description", value);
  }

  /**
   * Set `action`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `action`.
   * @returns This value.
   */
  action(value: Value<OperatorFields, "action">): this {
    return set(this, "action", value);
  }

  /**
   * Set `budget`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `budget`.
   * @returns This value.
   */
  budget(value: Value<OperatorFields, "budget">): this {
    return set(this, "budget", value);
  }
}
/**
 * A Verifier Card: the evidence it accepts, the steps that compute over it,
 * and the requirements that judge it.
 */
export class Verifier extends AuthoredCard {
  /**
   * An empty Verifier Card named `name` at `version`, with any spec fields
   * given in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: VerifierOptions = {}) {
    super(NativeAuthored.card("Verifier", name, version));
    apply(this, options);
  }

  /**
   * The step stages the server would execute, after validating this Verifier
   * as its YAML file would be. A step runs after every step it names in
   * `depends_on` and every step whose output or status it references; steps
   * with no edge between them share a stage.
   *
   * @returns The execution plan.
   * @throws `WyrdError` `WYRD_LOADER_400_INVALID_ENVELOPE` for a field that
   * does not decode, and the shared Verifier validation's refusal.
   */
  plan(): Wire.ExecutionPlan {
    return lifecycleValue<Wire.ExecutionPlan>(authoredOf(this).plan());
  }

  /**
   * Set `description`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `description`.
   * @returns This value.
   */
  description(value: Value<VerifierFields, "description">): this {
    return set(this, "description", value);
  }

  /**
   * Set `decide`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `decide`.
   * @returns This value.
   */
  decide(value: Value<VerifierFields, "decide">): this {
    return set(this, "decide", value);
  }

  /**
   * Set `context_capture`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `context_capture`.
   * @returns This value.
   */
  contextCapture(value: Value<VerifierFields, "context_capture">): this {
    return set(this, "context_capture", value);
  }

  /**
   * Set `order_by`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `order_by`.
   * @returns This value.
   */
  orderBy(value: Value<VerifierFields, "order_by">): this {
    return set(this, "order_by", value);
  }

  /**
   * Set the whole `steps` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  steps(items: Value<VerifierFields, "steps">): this {
    return set(this, "steps", items);
  }

  /**
   * Append one entry to `steps`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addStep(item: Item<VerifierFields, "steps">): this {
    return add(this, "steps", item);
  }

  /**
   * Set the whole `requirements` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  requirements(items: Value<VerifierFields, "requirements">): this {
    return set(this, "requirements", items);
  }

  /**
   * Append one entry to `requirements`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addRequirement(item: Item<VerifierFields, "requirements">): this {
    return add(this, "requirements", item);
  }

  /**
   * Set the whole `inputs` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  inputs(entries: Value<VerifierFields, "inputs">): this {
    return set(this, "inputs", entries);
  }

  /**
   * Insert the entry `key` into `inputs`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addInput(key: string, value: Entry<VerifierFields, "inputs">): this {
    return insert(this, "inputs", key, value);
  }
}

/**
 * An Artifact Card: a local file or directory published before the graph that
 * references it, or an Artifact defined only by its spec, declared in its
 * graph like any sibling Card.
 */
export class Artifact extends AuthoredCard {
  /**
   * An empty Artifact Card named `name` at `version`, with any spec fields
   * given in `options`; nothing is validated until its graph is.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param options - Spec fields, each set as its setter sets it.
   */
  constructor(name: string, version: string, options: ArtifactOptions = {}) {
    super(NativeAuthored.card("Artifact", name, version));
    apply(this, options);
  }

  /**
   * The `file` Artifact named `name` at `version`, published from the file or
   * directory at `path` before its graph; nothing is read until it is
   * published.
   *
   * @param name - The Card name.
   * @param version - The exact version.
   * @param path - The local file or directory.
   * @param options - Spec fields, each set as its setter sets it.
   * @returns The local Artifact.
   */
  static fromPath(name: string, version: string, path: string, options: ArtifactOptions = {}): Artifact {
    const artifact = adopt(Artifact.prototype, NativeAuthored.artifactFromPath(name, version, path));
    apply(artifact, options);
    return artifact;
  }

  /**
   * Set whether supported documents are extracted beside their originals when
   * the local source is published.
   *
   * @param extract - Whether to extract.
   * @returns This Artifact.
   */
  extract(extract: boolean): this {
    authoredOf(this).extract(extract);
    return this;
  }

  /**
   * Select the fixed file or directory prefix `path` of this exact Artifact
   * version; a local Artifact is published first.
   *
   * @param path - The file, or directory prefix.
   * @returns The selection.
   */
  file(path: string): ArtifactFile {
    return adopt(ArtifactFile.prototype, authoredOf(this).file(path));
  }

  /**
   * Set `artifact_kind`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `artifact_kind`.
   * @returns This value.
   */
  artifactKind(value: Value<ArtifactFields, "artifact_kind">): this {
    return set(this, "artifact_kind", value);
  }

  /**
   * Set `content_type`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `content_type`.
   * @returns This value.
   */
  contentType(value: Value<ArtifactFields, "content_type">): this {
    return set(this, "content_type", value);
  }

  /**
   * Set `size_bytes`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `size_bytes`.
   * @returns This value.
   */
  sizeBytes(value: Value<ArtifactFields, "size_bytes">): this {
    return set(this, "size_bytes", value);
  }

  /**
   * Set `integrity`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `integrity`.
   * @returns This value.
   */
  integrity(value: Value<ArtifactFields, "integrity">): this {
    return set(this, "integrity", value);
  }

  /**
   * Set `schema_ref`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `schema_ref`.
   * @returns This value.
   */
  schemaRef(value: Value<ArtifactFields, "schema_ref">): this {
    return set(this, "schema_ref", value);
  }

  /**
   * Set `framework_adapter`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `framework_adapter`.
   * @returns This value.
   */
  frameworkAdapter(value: Value<ArtifactFields, "framework_adapter">): this {
    return set(this, "framework_adapter", value);
  }

  /**
   * Set `external_uri`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `external_uri`.
   * @returns This value.
   */
  externalUri(value: Value<ArtifactFields, "external_uri">): this {
    return set(this, "external_uri", value);
  }

  /**
   * Set the whole `artifact_uris` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  artifactUris(items: Value<ArtifactFields, "artifact_uris">): this {
    return set(this, "artifact_uris", items);
  }

  /**
   * Append one entry to `artifact_uris`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addArtifactUri(item: Item<ArtifactFields, "artifact_uris">): this {
    return add(this, "artifact_uris", item);
  }

  /**
   * Set the whole `metadata` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  metadata(entries: Value<ArtifactFields, "metadata">): this {
    return set(this, "metadata", entries);
  }

  /**
   * Insert the entry `key` into `metadata`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addMetadata(key: string, value: Entry<ArtifactFields, "metadata">): this {
    return insert(this, "metadata", key, value);
  }
}

/** A Card of any registrable kind, authored as its kind and canonical spec JSON. */
export class CardDefinition extends AuthoredCard {
  /**
   * Define the `kind` Card `name` at `version` with the canonical `spec` JSON,
   * as its YAML `spec` would hold it.
   *
   * @param kind - The Card kind.
   * @param name - The Card name.
   * @param version - The exact version.
   * @param spec - The canonical spec.
   */
  constructor(kind: Wire.CardKind, name: string, version: string, spec: Wire.JsonValue) {
    super(NativeAuthored.definition(kind, name, version, argument("spec", spec)));
  }
}

/** One Service component: an alias for a referenced Card and the Verifiers that check it. */
export class Component {
  /**
   * The component `alias` for `ref`: a Card, an exact {@link CardRef}, or a
   * reference path, with any other fields given in `options`.
   *
   * @param alias - The component alias.
   * @param ref - The referenced Card.
   * @param options - Other fields, each set as its setter sets it.
   */
  constructor(alias: string, ref: Value<ComponentFields, "ref">, options: ComponentOptions = {}) {
    holdAuthored(this, NativeAuthored.component(alias, argument("ref", ref)));
    apply(this, options);
  }

  /**
   * Set `source`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `source`.
   * @returns This value.
   */
  source(value: Value<ComponentFields, "source">): this {
    return set(this, "source", value);
  }

  /**
   * Set the whole `verified_by` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  verifiedBy(items: Value<ComponentFields, "verified_by">): this {
    return set(this, "verified_by", items);
  }

  /**
   * Append one entry to `verified_by`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addVerifiedBy(item: Item<ComponentFields, "verified_by">): this {
    return add(this, "verified_by", item);
  }

  /**
   * Set the whole `credential_refs` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  credentialRefs(items: Value<ComponentFields, "credential_refs">): this {
    return set(this, "credential_refs", items);
  }

  /**
   * Append one entry to `credential_refs`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addCredentialRef(item: Item<ComponentFields, "credential_refs">): this {
    return add(this, "credential_refs", item);
  }

  /**
   * Set the whole `config` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  config(entries: Value<ComponentFields, "config">): this {
    return set(this, "config", entries);
  }

  /**
   * Insert the entry `key` into `config`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addConfig(key: string, value: Entry<ComponentFields, "config">): this {
    return insert(this, "config", key, value);
  }
}

/** One verification binding: the Verifier to run, when it runs, and the Operators that react to its verdicts. */
export class Binding {
  /**
   * Bind `verifier`: a {@link Verifier}, an exact {@link CardRef}, or a
   * reference path, with any other fields given in `options`.
   *
   * @param verifier - The Verifier.
   * @param options - Other fields, each set as its setter sets it.
   */
  constructor(verifier: Value<BindingFields, "verifier">, options: BindingOptions = {}) {
    holdAuthored(this, NativeAuthored.binding(argument("verifier", verifier)));
    apply(this, options);
  }

  /**
   * Set `runs_on`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `runs_on`.
   * @returns This value.
   */
  runsOn(value: Value<BindingFields, "runs_on">): this {
    return set(this, "runs_on", value);
  }

  /**
   * Set the whole `on_failure` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  onFailure(items: Value<BindingFields, "on_failure">): this {
    return set(this, "on_failure", items);
  }

  /**
   * Append one entry to `on_failure`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addOnFailure(item: Item<BindingFields, "on_failure">): this {
    return add(this, "on_failure", item);
  }

  /**
   * Set the whole `on_verdict_change` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  onVerdictChange(items: Value<BindingFields, "on_verdict_change">): this {
    return set(this, "on_verdict_change", items);
  }

  /**
   * Append one entry to `on_verdict_change`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addOnVerdictChange(item: Item<BindingFields, "on_verdict_change">): this {
    return add(this, "on_verdict_change", item);
  }
}

/** One data-producing Verifier step. */
export class Step {
  /**
   * The step `id` running `action`, with any other fields given in `options`.
   *
   * @param id - The step ID.
   * @param action - An {@link Action} or its canonical object.
   * @param options - Other fields, each set as its setter sets it.
   */
  constructor(id: string, action: Value<StepFields, "action">, options: StepOptions = {}) {
    holdAuthored(this, NativeAuthored.step(id, argument("action", action)));
    apply(this, options);
  }

  /**
   * Set `when`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `when`.
   * @returns This value.
   */
  when(value: Value<StepFields, "when">): this {
    return set(this, "when", value);
  }

  /**
   * Set `timeout_seconds`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `timeout_seconds`.
   * @returns This value.
   */
  timeoutSeconds(value: Value<StepFields, "timeout_seconds">): this {
    return set(this, "timeout_seconds", value);
  }

  /**
   * Set `retries`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `retries`.
   * @returns This value.
   */
  retries(value: Value<StepFields, "retries">): this {
    return set(this, "retries", value);
  }

  /**
   * Set the whole `depends_on` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  dependsOn(items: Value<StepFields, "depends_on">): this {
    return set(this, "depends_on", items);
  }

  /**
   * Append one entry to `depends_on`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addDependsOn(item: Item<StepFields, "depends_on">): this {
    return add(this, "depends_on", item);
  }

  /**
   * Set the whole `requirements` list, replacing every earlier entry and whatever it referenced.
   *
   * @param items - The new entries.
   * @returns This value.
   */
  requirements(items: Value<StepFields, "requirements">): this {
    return set(this, "requirements", items);
  }

  /**
   * Append one entry to `requirements`; a repeated ID is refused at validation.
   *
   * @param item - The entry.
   * @returns This value.
   */
  addRequirement(item: Item<StepFields, "requirements">): this {
    return add(this, "requirements", item);
  }
}

/** One requirement, producing exactly one verdict. */
export class Requirement {
  /**
   * The requirement `id` claiming `statement`, judged by `check`.
   *
   * @param id - The requirement ID.
   * @param statement - The claim.
   * @param check - A {@link Check} or its canonical object.
   */
  constructor(id: string, statement: string, check: Authorable<Wire.Check>) {
    holdAuthored(this, NativeAuthored.requirement(id, statement, argument("check", check)));
  }
}

/**
 * One named evidence slot of a Verifier. Evidence a caller omits stays
 * missing, never `null`; a `json` slot's schema alone decides whether a
 * supplied `null` is admitted.
 */
export class InputSlot {
  private constructor() {}

  /**
   * A JSON value constrained by the JSON Schema `schema`.
   *
   * @param schema - The JSON Schema.
   * @returns The slot.
   */
  static json(schema: Wire.JsonValue): InputSlot {
    return adopt(InputSlot.prototype, NativeAuthored.slotJson(argument("schema", schema)));
  }

  /**
   * A pinned file in one of `formats`, such as `"pdf"`.
   *
   * @param formats - The accepted formats.
   * @param options - Other fields, each set as its setter sets it.
   * @returns The slot.
   */
  static media(formats: Wire.TypedInputSlotMedia["formats"], options: MediaSlotOptions = {}): InputSlot {
    const slot = adopt(InputSlot.prototype, NativeAuthored.slotMedia(argument("formats", formats)));
    apply(slot, options);
    return slot;
  }

  /**
   * A trace resolved through tenant-scoped trace reads.
   *
   * @returns The slot.
   */
  static trace(): InputSlot {
    return adopt(InputSlot.prototype, NativeAuthored.slotTrace());
  }

  /**
   * Recorded measurements of `features` for a scheduled window.
   *
   * @param features - The measured features.
   * @returns The slot.
   */
  static samples(features: Wire.TypedInputSlotSamples["features"]): InputSlot {
    return adopt(InputSlot.prototype, NativeAuthored.slotSamples(argument("features", features)));
  }

  /**
   * A group Verifier's input authored as the source reference `text`, such as
   * `verifiers.<name>.inputs.<slot>`.
   *
   * @param text - The source reference.
   * @returns The slot.
   */
  static source(text: string): InputSlot {
    return adopt(InputSlot.prototype, NativeAuthored.slotSource(text));
  }

  /**
   * Set `max_bytes`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `max_bytes`.
   * @returns This value.
   */
  maxBytes(value: Value<InputSlotFields, "max_bytes">): this {
    return set(this, "max_bytes", value);
  }
}

/** A step action: extract, an Agent call, or sandboxed code. */
export class Action {
  private constructor() {}

  /**
   * Extract text and structure from the media slot `from`, such as
   * `inputs.report`.
   *
   * @param from - The media slot reference.
   * @returns The action.
   */
  static extract(from: Wire.ActionExtract["from"]): Action {
    return adopt(Action.prototype, NativeAuthored.actionExtract(from));
  }

  /**
   * Invoke `agent`: an {@link Agent}, an exact {@link CardRef}, a reference
   * path, or inline Agent JSON.
   *
   * @param agent - The Agent.
   * @param options - Other fields, each set as its setter sets it.
   * @returns The action.
   */
  static agent(agent: Authorable<Wire.ActionAgent["agent"]>, options: AgentActionOptions = {}): Action {
    const action = adopt(Action.prototype, NativeAuthored.actionAgent(argument("agent", agent)));
    apply(action, options);
    return action;
  }

  /**
   * Run `entrypoint` of `source` as sandboxed code returning JSON data.
   *
   * @param source - A {@link CodeSource}, {@link ArtifactFile}, or canonical source.
   * @param entrypoint - The function to run.
   * @param options - Other fields, each set as its setter sets it.
   * @returns The action.
   */
  static code(
    source: Authorable<Wire.ActionCode["source"]>,
    entrypoint: string,
    options: CodeActionOptions = {},
  ): Action {
    const action = adopt(Action.prototype, NativeAuthored.actionCode(argument("source", source), entrypoint));
    apply(action, options);
    return action;
  }

  /**
   * Set `runtime`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `runtime`.
   * @returns This value.
   */
  runtime(value: Value<ActionFields, "runtime">): this {
    return set(this, "runtime", value);
  }

  /**
   * Set the whole `inputs` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  inputs(entries: Value<ActionFields, "inputs">): this {
    return set(this, "inputs", entries);
  }

  /**
   * Insert the entry `key` into `inputs`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  bindInput(key: string, value: Entry<ActionFields, "inputs">): this {
    return insert(this, "inputs", key, value);
  }

  /**
   * Set the whole `workspace` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  workspace(entries: Value<ActionFields, "workspace">): this {
    return set(this, "workspace", entries);
  }

  /**
   * Insert the entry `key` into `workspace`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addWorkspace(key: string, value: Entry<ActionFields, "workspace">): this {
    return insert(this, "workspace", key, value);
  }
}

/** A verdict-producing requirement check. */
export class Check {
  private constructor() {}

  /**
   * Compare the one value at `path` with `expected` by `operator`, such as
   * `"equals"` or its object form.
   *
   * @param path - The compared reference.
   * @param operator - The comparison operator.
   * @param expected - The expected value.
   * @returns The check.
   */
  static assert(path: string, operator: Wire.CheckAssert["operator"], expected: Wire.JsonValue): Check {
    return adopt(
      Check.prototype,
      NativeAuthored.checkAssert(path, argument("operator", operator), argument("expected", expected)),
    );
  }

  /**
   * Compare every value `each` selects through `[*]` with `expected` by
   * `operator`.
   *
   * @param each - The reference selecting each compared value.
   * @param operator - The comparison operator.
   * @param expected - The expected value.
   * @returns The check.
   */
  static each(each: string, operator: Wire.CheckAssert["operator"], expected: Wire.JsonValue): Check {
    return adopt(
      Check.prototype,
      NativeAuthored.checkEach(each, argument("operator", operator), argument("expected", expected)),
    );
  }

  /**
   * Compare the spans `span_selector` selects in the trace slot `on` with
   * `expected` by `operator`.
   *
   * @param on - The trace slot reference.
   * @param spanSelector - The span selector.
   * @param operator - The comparison operator.
   * @param expected - The expected value.
   * @returns The check.
   */
  static trace(
    on: string,
    spanSelector: Wire.CheckTrace["span_selector"],
    operator: Wire.CheckTrace["operator"],
    expected: Wire.JsonValue,
  ): Check {
    return adopt(
      Check.prototype,
      NativeAuthored.checkTrace(on, spanSelector, argument("operator", operator), argument("expected", expected)),
    );
  }

  /**
   * Judge with `agent`, which returns the Verdict schema.
   *
   * @param agent - The Agent.
   * @param options - Other fields, each set as its setter sets it.
   * @returns The check.
   */
  static agent(agent: Authorable<Wire.CheckAgent["agent"]>, options: AgentCheckOptions = {}): Check {
    const check = adopt(Check.prototype, NativeAuthored.checkAgent(argument("agent", agent)));
    apply(check, options);
    return check;
  }

  /**
   * Judge with `entrypoint` of `source`, which returns the Verdict schema.
   *
   * @param source - A {@link CodeSource}, {@link ArtifactFile}, or canonical source.
   * @param entrypoint - The function to run.
   * @param options - Other fields, each set as its setter sets it.
   * @returns The check.
   */
  static code(source: Authorable<Wire.CheckCode["source"]>, entrypoint: string, options: CodeCheckOptions = {}): Check {
    const check = adopt(Check.prototype, NativeAuthored.checkCode(argument("source", source), entrypoint));
    apply(check, options);
    return check;
  }

  /**
   * Judge statistical drift over the samples slot `on` with the drift fields
   * `spec`: method, signal, condition, and profile.
   *
   * @param on - The samples slot reference.
   * @param spec - The drift fields.
   * @returns The check.
   */
  static drift(on: string, spec: Omit<Wire.CheckDrift, "kind" | "on">): Check {
    return adopt(Check.prototype, NativeAuthored.checkDrift(on, argument("spec", spec)));
  }

  /**
   * Set `runtime`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `runtime`.
   * @returns This value.
   */
  runtime(value: Value<CheckFields, "runtime">): this {
    return set(this, "runtime", value);
  }

  /**
   * Set `response_format`, replacing its earlier value and whatever it referenced.
   *
   * @param value - The new `response_format`.
   * @returns This value.
   */
  responseFormat(value: Value<CheckFields, "response_format">): this {
    return set(this, "response_format", value);
  }

  /**
   * Set the whole `inputs` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  inputs(entries: Value<CheckFields, "inputs">): this {
    return set(this, "inputs", entries);
  }

  /**
   * Insert the entry `key` into `inputs`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  bindInput(key: string, value: Entry<CheckFields, "inputs">): this {
    return insert(this, "inputs", key, value);
  }

  /**
   * Set the whole `workspace` map, replacing every earlier entry and whatever it referenced.
   *
   * @param entries - The new entries.
   * @returns This value.
   */
  workspace(entries: Value<CheckFields, "workspace">): this {
    return set(this, "workspace", entries);
  }

  /**
   * Insert the entry `key` into `workspace`; a repeated key is refused at validation.
   *
   * @param key - The entry's key.
   * @param value - The entry's value.
   * @returns This value.
   */
  addWorkspace(key: string, value: Entry<CheckFields, "workspace">): this {
    return insert(this, "workspace", key, value);
  }
}

/**
 * An exact Card reference: kind, name, and one version, kept as authored until
 * validation. At a reference position it stays an external reference to a
 * registered Card and nothing is uploaded for it; registration pins its UID.
 */
export class CardRef {
  /**
   * The exact reference to the `kind` Card `name` at `version`.
   *
   * @param kind - The referenced kind.
   * @param name - The referenced name.
   * @param version - The referenced exact version.
   */
  constructor(kind: Wire.CardKind, name: string, version: string) {
    holdAuthored(this, NativeAuthored.reference(kind, name, version));
  }

  /** The exact reference to the Data Card `name` at `version`. */
  static data(name: string, version: string): CardRef {
    return new CardRef("Data", name, version);
  }

  /** The exact reference to the Model Card `name` at `version`. */
  static model(name: string, version: string): CardRef {
    return new CardRef("Model", name, version);
  }

  /** The exact reference to the Artifact Card `name` at `version`. */
  static artifact(name: string, version: string): CardRef {
    return new CardRef("Artifact", name, version);
  }

  /** The exact reference to the Experiment Card `name` at `version`. */
  static experiment(name: string, version: string): CardRef {
    return new CardRef("Experiment", name, version);
  }

  /** The exact reference to the Prompt Card `name` at `version`. */
  static prompt(name: string, version: string): CardRef {
    return new CardRef("Prompt", name, version);
  }

  /** The exact reference to the Agent Card `name` at `version`. */
  static agent(name: string, version: string): CardRef {
    return new CardRef("Agent", name, version);
  }

  /** The exact reference to the Mcp Card `name` at `version`. */
  static mcp(name: string, version: string): CardRef {
    return new CardRef("Mcp", name, version);
  }

  /** The exact reference to the Service Card `name` at `version`. */
  static service(name: string, version: string): CardRef {
    return new CardRef("Service", name, version);
  }

  /** The exact reference to the Audit Card `name` at `version`. */
  static audit(name: string, version: string): CardRef {
    return new CardRef("Audit", name, version);
  }

  /** The exact reference to the Verifier Card `name` at `version`. */
  static verifier(name: string, version: string): CardRef {
    return new CardRef("Verifier", name, version);
  }

  /** The exact reference to the Trigger Card `name` at `version`. */
  static trigger(name: string, version: string): CardRef {
    return new CardRef("Trigger", name, version);
  }

  /** The exact reference to the Operator Card `name` at `version`. */
  static operator(name: string, version: string): CardRef {
    return new CardRef("Operator", name, version);
  }

  /** The referenced kind. */
  get kind(): Wire.CardKind {
    return authoredOf(this).identity().kind as Wire.CardKind;
  }

  /** The referenced name, as authored. */
  get name(): string {
    return authoredOf(this).identity().name;
  }

  /** The referenced version, as authored. */
  get version(): string {
    return authoredOf(this).identity().version;
  }

  /**
   * Select the fixed file or directory prefix `path` of this registered
   * Artifact version, with no local source and nothing uploaded.
   *
   * @param path - The file, or directory prefix.
   * @returns The selection.
   */
  file(path: string): ArtifactFile {
    return adopt(ArtifactFile.prototype, authoredOf(this).file(path));
  }
}

/**
 * A fixed file or directory prefix of one exact Artifact version, mounted into
 * a workspace or run as code; built by {@link Artifact.file} or
 * {@link CardRef.file}. A local Artifact's selection publishes that Artifact
 * before its graph.
 */
export class ArtifactFile {
  private constructor() {}
}

/**
 * The source of a code action or check: inline text, or a local file captured
 * as inline text when its graph is validated. An {@link ArtifactFile} is the
 * third canonical form.
 */
export class CodeSource {
  private constructor() {}

  /**
   * Run `text`, embedded in the Card as `{inline: text}`.
   *
   * @param text - The source text.
   * @returns The source.
   */
  static inline(text: string): CodeSource {
    return adopt(CodeSource.prototype, NativeAuthored.codeInline(text));
  }

  /**
   * Run the UTF-8 file at `path`, read when the graph is validated or declared
   * and embedded as `{inline: text}`; nothing is read now, and each snapshot
   * reads the file afresh.
   *
   * @param path - The local file.
   * @returns The source.
   */
  static file(path: string): CodeSource {
    return adopt(CodeSource.prototype, NativeAuthored.codeFile(path));
  }
}
