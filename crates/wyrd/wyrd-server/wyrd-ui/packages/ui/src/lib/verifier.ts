/**
 * Verifier definition loaders: one exact Verifier Card read, projected into
 * the declared graph its page draws, and the bindings that run that exact
 * version. Topology comes from the references the spec already carries; no
 * read beyond exact Card reads is planned.
 */
import type { CardTypes } from 'wyrd-ai';
import { failed, wyrdProblem, type CardRef, type Executor, type Loaded } from './contract.js';

type VerifierSpec = CardTypes.VerifierSpec;
type Requirement = CardTypes.Requirement;

interface Context {
  readonly executor: Executor;
  readonly signal?: AbortSignal;
}

/** A declared evidence slot. `key` is `input:<slot>`. */
export interface DefinitionInput {
  readonly key: string;
  readonly id: string;
  /** The slot type, or `source` for a group source. */
  readonly kind: string;
  readonly detail: string;
}

/** A declared requirement. `key` is `req:<id>`; `step` names the step it is attached to. */
export interface DefinitionRequirement {
  readonly key: string;
  readonly id: string;
  readonly kind: string;
  readonly statement: string;
  readonly step?: string;
}

/** A declared step. `key` is `step:<id>`. */
export interface DefinitionStep {
  readonly key: string;
  readonly id: string;
  readonly kind: string;
  readonly detail: string;
  /** Ids of the steps this one waits for, in declaration order. */
  readonly needs: readonly string[];
  /** Declared gate and repetition, as short labels. */
  readonly chips: readonly string[];
  readonly requirements: readonly DefinitionRequirement[];
}

/** One declared dependency between node keys; `label` names a step output: the argument it is passed as, else the field read. */
export interface DefinitionEdge {
  readonly source: string;
  readonly target: string;
  readonly label?: string;
}

/** The declared graph of one Verifier version: no execution state. */
export interface DefinitionGraph {
  readonly inputs: readonly DefinitionInput[];
  readonly steps: readonly DefinitionStep[];
  /** Whole-Verifier requirements, judged after every step. */
  readonly requirements: readonly DefinitionRequirement[];
  readonly edges: readonly DefinitionEdge[];
}

/** `inputs.<slot>` or `steps.<id>`, with the output field when one is read. */
const REFERENCE = /^(inputs|steps)\.([^.[]+)(?:\.output\.([^.[]+))?/;

/** One reference a block reads, and the argument name it is passed as when it has one. */
type Reference = readonly [reference: string, argument?: string];

/** Every reference a step action or requirement check reads. */
function reads(block: CardTypes.Action | CardTypes.Check): Reference[] {
  switch (block.kind) {
    case 'extract':
      return [[block.from]];
    case 'trace':
    case 'drift':
      return [[block.on]];
    case 'assert':
      return [block.path, block.each].filter((each) => typeof each === 'string').map((each) => [each]);
    case 'agent':
    case 'code': {
      const workspace = 'workspace' in block ? Object.values(block.workspace ?? {}) : [];
      return [...Object.entries(block.inputs ?? {}).map(([argument, reference]): Reference => [reference, argument]), ...workspace.filter((each) => typeof each === 'string').map((each): Reference => [each])];
    }
  }
}

function inputDetail(slot: CardTypes.InputSlot): Pick<DefinitionInput, 'kind' | 'detail'> {
  if (typeof slot === 'string') return { kind: 'source', detail: slot };
  switch (slot.type) {
    case 'json':
      return { kind: 'json', detail: slot.schema ? 'JSON matching its schema' : 'any JSON value' };
    case 'media':
      return { kind: 'media', detail: slot.formats.join(', ') };
    case 'trace':
      return { kind: 'trace', detail: 'one assembled trace' };
    case 'samples':
      return { kind: 'samples', detail: slot.features.join(', ') };
  }
}

function stepDetail(action: CardTypes.Action): string {
  if (action.kind === 'extract') return `from ${action.from}`;
  if (action.kind === 'code') return `${action.entrypoint}()`;
  const agent = action.agent;
  if (typeof agent === 'string') return agent;
  return 'name' in agent && 'version' in agent ? `${agent.name}@${agent.version}` : 'inline Agent';
}

const requirement = (declared: Requirement, step?: string): DefinitionRequirement => ({
  key: `req:${declared.id}`,
  id: declared.id,
  kind: declared.check.kind,
  statement: declared.statement,
  ...(step === undefined ? {} : { step })
});

/**
 * Project a Verifier spec into its declared graph. A step depends on what its
 * action, gate and attached requirements reference plus `depends_on`; a
 * whole-Verifier requirement depends on what its check references.
 */
export function definitionGraph(spec: VerifierSpec): DefinitionGraph {
  const steps = spec.steps ?? [];
  const known = new Set([...Object.keys(spec.inputs ?? {}).map((id) => `input:${id}`), ...steps.map((step) => `step:${step.id}`)]);
  const edges = new Map<string, DefinitionEdge>();
  const link = (source: string, target: string, label?: string) => {
    if (source === target || !known.has(source)) return;
    const id = `${source}->${target}`;
    const label_ = edges.get(id)?.label ?? label;
    edges.set(id, { source, target, ...(label_ === undefined ? {} : { label: label_ }) });
  };
  const follow = (references: readonly Reference[], target: string) => {
    for (const [reference, argument] of references) {
      const match = REFERENCE.exec(reference);
      if (!match) continue;
      if (match[1] === 'inputs') link(`input:${match[2]}`, target);
      else link(`step:${match[2]}`, target, argument ?? match[3]);
    }
  };
  for (const step of steps) {
    const key = `step:${step.id}`;
    follow([...reads(step.action), ...(step.when ? [[step.when.path] as const] : []), ...(step.requirements ?? []).flatMap((each) => reads(each.check))], key);
    for (const id of step.depends_on ?? []) link(`step:${id}`, key);
  }
  for (const each of spec.requirements ?? []) follow(reads(each.check), `req:${each.id}`);
  const all = [...edges.values()];
  return {
    inputs: Object.entries(spec.inputs ?? {}).map(([id, slot]) => ({ key: `input:${id}`, id, ...inputDetail(slot) })),
    steps: steps.map((step) => ({
      key: `step:${step.id}`,
      id: step.id,
      kind: step.action.kind,
      detail: stepDetail(step.action),
      needs: all.filter((edge) => edge.target === `step:${step.id}` && edge.source.startsWith('step:')).map((edge) => edge.source.slice(5)),
      chips: [...(step.when ? ['when'] : []), ...(step.retries ? [`↻ ≤ ${step.retries} ${step.retries === 1 ? 'retry' : 'retries'}`] : [])],
      requirements: (step.requirements ?? []).map((each) => requirement(each, step.id))
    })),
    requirements: (spec.requirements ?? []).map((each) => requirement(each)),
    edges: all
  };
}

/** What `VerifierDefinition` renders: the authored spec and its declared graph. */
export interface VerifierDefinitionData {
  readonly spec: VerifierSpec;
  readonly graph: DefinitionGraph;
}

/** Props of `VerifierDefinition`. */
export interface VerifierDefinitionProps {
  readonly input: { readonly cardRef: CardRef };
  readonly result: Loaded<VerifierDefinitionData>;
}

type Read<T> = Extract<Loaded<T>, { state: 'populated' | 'denied' | 'error' }>;

/** Read one exact Card and refuse any kind but Verifier. */
async function readVerifier(ref: CardRef, context: Context) {
  const [read] = await context.executor.run([{ op: 'cards.get_by_ref', card_ref: ref }], context.signal);
  if (!read.ok) return failed(read.problem);
  if (read.value.kind !== 'Verifier')
    return { state: 'error' as const, problem: wyrdProblem('validation', `${ref.name}@${ref.version} is a ${read.value.kind} Card, not a Verifier`) };
  return { state: 'populated' as const, data: read.value };
}

/** Load an exact Verifier's authored definition and declared graph. */
export async function loadVerifierDefinition(input: { readonly cardRef: CardRef }, context: Context): Promise<VerifierDefinitionProps> {
  const read = await readVerifier(input.cardRef, context);
  if (read.state !== 'populated') return { input, result: read };
  const spec = read.data.spec as VerifierSpec;
  return { input, result: { state: 'populated', data: { spec, graph: definitionGraph(spec) } } };
}

/** One binding that runs the exact Verifier version. */
export interface VerifierBinding {
  /** The component alias the binding is attached to; absent on the owner itself. */
  readonly subject?: string;
  /** When the binding runs, as authored: a Trigger reference, a schedule, a group end, or explicit invocation only. */
  readonly when: string;
}

/** One Card whose spec binds the Verifier, and its bindings or the refusal that replaced them. */
export interface VerifierOwner {
  readonly ref: CardRef;
  readonly bindings: Read<readonly VerifierBinding[]>;
}

/** Props of `RunsOn`. */
export interface RunsOnProps {
  readonly input: { readonly cardRef: CardRef };
  readonly result: Loaded<readonly VerifierOwner[]>;
}

/** The most owner Cards one `RunsOn` reads. */
const OWNER_READS = 50;

function runsWhen(runsOn: CardTypes.VerificationBinding['runs_on']): string {
  if (runsOn === null || runsOn === undefined) return 'explicit invocation only';
  if (typeof runsOn === 'string') return runsOn;
  if ('cron' in runsOn) return `schedule ${runsOn.cron}${runsOn.tz ? ` ${runsOn.tz}` : ''}`;
  if ('after' in runsOn) return `group end after ${runsOn.after}`;
  return 'name' in runsOn && 'version' in runsOn ? `Trigger ${runsOn.name}@${runsOn.version}` : 'sibling Trigger';
}

/** The bindings in an owner's spec that pin exactly `verifier`. */
function bindingsOf(spec: unknown, verifier: CardRef): VerifierBinding[] {
  const owner = spec as { verified_by?: readonly CardTypes.VerificationBinding[]; components?: readonly CardTypes.ServiceComponent[] };
  const pins = (binding: CardTypes.VerificationBinding) =>
    typeof binding.verifier === 'object' && 'version' in binding.verifier && binding.verifier.name === verifier.name && binding.verifier.version === verifier.version;
  const own = (owner.verified_by ?? []).filter(pins).map((binding) => ({ when: runsWhen(binding.runs_on) }));
  const components = (owner.components ?? []).flatMap((component) =>
    (component.verified_by ?? []).filter(pins).map((binding) => ({ subject: component.alias, when: runsWhen(binding.runs_on) }))
  );
  return [...own, ...components];
}

/**
 * Load where an exact Verifier version runs: the Services and Agents that
 * reference it, from its inbound relationships, each read for its bindings.
 * A refused Verifier read issues no owner read; a refused owner keeps its row.
 */
export async function loadRunsOn(input: { readonly cardRef: CardRef }, context: Context): Promise<RunsOnProps> {
  const read = await readVerifier(input.cardRef, context);
  if (read.state !== 'populated') return { input, result: read };
  const refs = (read.data.relationships?.inbound_refs ?? []).map((edge) => edge.ref).filter((ref) => ref.kind === 'Service' || ref.kind === 'Agent');
  if (refs.length === 0) return { input, result: { state: 'empty', data: [], reason: 'No binding runs this exact version.' } };
  const owners = await context.executor.run(
    refs.slice(0, OWNER_READS).map((ref) => ({ op: 'cards.get_by_ref', card_ref: ref }) as const),
    context.signal
  );
  const data = refs.map((ref, index): VerifierOwner => {
    const owner = owners[index];
    if (owner === undefined) return { ref, bindings: { state: 'error', problem: wyrdProblem('validation', `RunsOn reads at most ${OWNER_READS} Cards`) } };
    if (!owner.ok) return { ref, bindings: failed(owner.problem) };
    return { ref, bindings: { state: 'populated', data: bindingsOf(owner.value.spec, input.cardRef) } };
  });
  return { input, result: { state: 'populated', data } };
}
