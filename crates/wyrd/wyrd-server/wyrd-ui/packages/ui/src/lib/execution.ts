/**
 * Exact verification execution loader: the execution's recorded status, the
 * declared graph of the Verifier it ran, its bounded invocation and
 * publication pages, and whatever the selected node needs. Each part is read
 * and refused on its own; nothing is recomputed, awaited or resubmitted.
 */
import type { CardTypes, VerificationExecutionState } from 'wyrd-ai';
import { failed, wyrdProblem, type CardRef, type Executor, type Loaded, type Problem, type PublicationText, type QueryResult, type WireRequest, type WireResult } from './contract.js';
import { fmtBytes } from './format.js';
import type { ExecutionInput } from './schemas.js';
import { definitionGraph, type DefinitionGraph } from './verifier.js';

interface Context {
  readonly executor: Executor;
  readonly signal?: AbortSignal;
}

/** One part of an execution: read, refused, or not available for a stated reason. */
export type Part<T> = Extract<Loaded<T>, { state: 'populated' | 'denied' | 'error' }> | { readonly state: 'empty'; readonly reason: string };

/** The exact Verifier an execution ran, as registered. */
export interface ExecutionDefinition {
  readonly ref: CardRef;
  readonly spec: CardTypes.VerifierSpec;
  readonly graph: DefinitionGraph;
}

/** One pinned member of a session source slot, in the recorded order. */
export interface SessionMember {
  readonly execution_id: string;
  /** The member's own recorded status; absent beyond the read bound. */
  readonly status?: Part<VerificationExecutionState>;
}

/** One recorded drift result of the same binding. */
export interface DriftPoint {
  readonly execution_id: string;
  readonly ended_at: string;
  readonly window_start: string | null;
  readonly window_end: string | null;
  /** Recorded score per feature; null where the feature was inconclusive or absent. */
  readonly scores: Readonly<Record<string, number | null>>;
}

/** The recorded window of a drift judgment and its binding's bounded history, oldest first. */
export interface DriftContext {
  readonly window_start: string | null;
  readonly window_end: string | null;
  readonly history: readonly DriftPoint[];
}

/** What `VerificationExecution` renders. */
export interface ExecutionData {
  readonly status: VerificationExecutionState;
  readonly definition: Part<ExecutionDefinition>;
  /** Recorded progress, read only when the Verifier has steps or no result is recorded yet. */
  readonly invocations?: Part<CardTypes.InvocationPage>;
  /** Retained outputs, read only when the outputs or a step are selected. */
  readonly publications?: Part<CardTypes.PublicationPage>;
  /** Completed records, read only when asked for. */
  readonly details?: Part<CardTypes.VerificationDetails>;
  /** The selected publication's body or manifest. */
  readonly publication?: Part<CardTypes.Publication>;
  /** The selected file publication's text, when it is one file within the preview bound. */
  readonly text?: Part<PublicationText>;
  /** The pinned members of the selected session source slot. */
  readonly members?: readonly SessionMember[];
  /** The accepted input declarations, read when an input is selected. */
  readonly inputs?: Part<unknown>;
  /** The drift context of the selected drift requirement. */
  readonly drift?: Part<DriftContext>;
  /** The Repository Card a pull request execution's retained input names; absent for other origins and until that input is visible. */
  readonly repository?: string;
}

/** Props of `VerificationExecution`. */
export interface VerificationExecutionProps {
  readonly input: ExecutionInput;
  readonly result: Loaded<ExecutionData>;
}

/** The most invocations or publications one page reads; the server's own maximum. */
export const EXECUTION_PAGE = 200;
/** The most session members whose own status one load reads. */
export const MEMBER_READS = 50;
/** The most earlier results a drift history reads. */
export const DRIFT_HISTORY = 30;
/** The most bytes of one retained file read for a text preview. */
export const TEXT_PREVIEW = 65_536;

const TERMINAL: readonly VerificationExecutionState['status'][] = ['completed', 'cancelled', 'timed_out', 'errored'];
/** Whether the execution has reached a terminal lifecycle status. */
export const settled = (status: VerificationExecutionState['status']) => TERMINAL.includes(status);

const part = <T>(read: WireResult, value: (value: never) => T = (each) => each as T): Part<T> =>
  read.ok ? { state: 'populated', data: value(read.value as never) } : failed(read.problem);

/** The server's answer for an execution that ran directly: it has a result and no queued record, so no progress or outputs. */
const NOT_QUEUED = 'WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND';

const VERIFIER_UID = 'SELECT verifier_card_uid FROM bifrost.verification.execution_inputs WHERE execution_id = $1 LIMIT 1';
const REPOSITORY = 'SELECT subject_name FROM bifrost.verification.execution_inputs WHERE execution_id = $1 LIMIT 1';
const INPUTS = 'SELECT to_json(inputs) AS inputs FROM bifrost.verification.execution_inputs WHERE execution_id = $1 LIMIT 1';
const DRIFT_ROW = 'SELECT binding_id, window_start, window_end, ended_at FROM bifrost.verification.results WHERE execution_id = $1 LIMIT 1';
const DRIFT_ROWS =
  'SELECT r.execution_id, r.ended_at, r.window_start, r.window_end, to_json(o.output) AS output ' +
  'FROM bifrost.verification.results r JOIN bifrost.verification.execution_outputs o ON o.result_id = r.result_id ' +
  `WHERE r.binding_id = $1 AND r.ended_at <= $2::timestamptz ORDER BY r.ended_at DESC LIMIT ${DRIFT_HISTORY}`;

/** A Variant column arrives as JSON text or as the value itself. */
function json(value: unknown): unknown {
  if (typeof value !== 'string') return value;
  try {
    return JSON.parse(value);
  } catch {
    return value;
  }
}

const text = (value: unknown) => (value === null || value === undefined ? null : String(value));

/** The Verifier an execution ran: the result's exact reference, else the retained UID through the existing UID Card read. */
async function readDefinition(state: VerificationExecutionState, one: (request: WireRequest) => Promise<WireResult>): Promise<Part<ExecutionDefinition>> {
  let read: WireResult;
  if (state.result) read = await one({ op: 'cards.get_by_ref', card_ref: state.result.verifier });
  else {
    const metadata = await one({ op: 'bifrost.query', sql: VERIFIER_UID, params: [state.execution_id], max_rows: 1 });
    if (!metadata.ok) return failed(metadata.problem);
    const uid = (metadata.value as QueryResult).rows[0]?.[0];
    if (typeof uid !== 'string')
      return { state: 'empty', reason: 'The Verifier this execution runs is not yet visible in retained metadata. Refresh to read it again.' };
    read = await one({ op: 'cards.get_by_uid', kind: 'Verifier', card_uid: uid });
  }
  if (!read.ok) return failed(read.problem);
  const card = read.value as CardTypes.RegisteredCard;
  if (card.kind !== 'Verifier') return { state: 'error', problem: wyrdProblem('validation', `${card.metadata.name}@${card.metadata.version} is a ${card.kind} Card, not a Verifier`) };
  const spec = card.spec as CardTypes.VerifierSpec;
  const ref = { kind: 'Verifier', name: card.metadata.name, version: card.metadata.version, uid: card.metadata.uid } as CardRef;
  return { state: 'populated', data: { ref, spec, graph: definitionGraph(spec) } };
}

/** The recorded window of this execution's result and the earlier results of its binding. */
async function readDrift(id: string, requirement: string, one: (request: WireRequest) => Promise<WireResult>): Promise<Part<DriftContext>> {
  const own = await one({ op: 'bifrost.query', sql: DRIFT_ROW, params: [id], max_rows: 1 });
  if (!own.ok) return failed(own.problem);
  const row = (own.value as QueryResult).rows[0];
  if (!row) return { state: 'empty', reason: 'The retained result of this execution is not yet visible. Refresh to read it again.' };
  const [binding, window_start, window_end, ended] = [text(row[0]), text(row[1]), text(row[2]), text(row[3])];
  if (binding === null || ended === null) return { state: 'populated', data: { window_start, window_end, history: [] } };
  const rows = await one({ op: 'bifrost.query', sql: DRIFT_ROWS, params: [binding, ended], max_rows: DRIFT_HISTORY });
  if (!rows.ok) return failed(rows.problem);
  const history = (rows.value as QueryResult).rows.map((each): DriftPoint => {
    const output = json(each[4]) as { requirements?: readonly CardTypes.RequirementResult[] } | null;
    const features = output?.requirements?.find((judged) => judged.id === requirement)?.detail?.drift?.features ?? {};
    return {
      execution_id: String(each[0]),
      ended_at: String(each[1]),
      window_start: text(each[2]),
      window_end: text(each[3]),
      scores: Object.fromEntries(Object.entries(features).map(([name, feature]) => [name, feature.score]))
    };
  });
  return { state: 'populated', data: { window_start, window_end, history: history.reverse() } };
}

/**
 * Read one publication of one producer attempt, then the text of a single
 * retained file within {@link TEXT_PREVIEW}. A larger file is never read; a
 * directory is only its manifest.
 */
async function readPublication(
  selector: { readonly execution_id: string; readonly step_id: string; readonly name: string } & CardTypes.PublicationSelector,
  one: (request: WireRequest) => Promise<WireResult>
): Promise<{ publication: Part<CardTypes.Publication>; text?: Part<PublicationText> }> {
  const publication = part<CardTypes.Publication>(await one({ op: 'verification.get_publication', ...selector }));
  if (publication.state !== 'populated' || publication.data.kind !== 'file' || publication.data.manifest.files.length !== 1) return { publication };
  if (publication.data.manifest.files[0].size_bytes > TEXT_PREVIEW)
    return { publication, text: { state: 'empty', reason: `This file is larger than the ${fmtBytes(TEXT_PREVIEW)} preview bound. Download it to read it.` } };
  return { publication, text: part<PublicationText>(await one({ op: 'verification.read_publication_text', ...selector, max_bytes: TEXT_PREVIEW })) };
}

/**
 * Load one exact execution. A refused or missing status ends the load with
 * that outcome and no further read. Otherwise the definition is read, the
 * invocation page only when the Verifier has steps, a step is selected or no
 * result is recorded yet, and the selection adds only what it names: the publication page for
 * the outputs or a step, completed details when asked for and the
 * execution has a result, one publication, one source slot's members, the
 * accepted inputs, one drift requirement's recorded context, or a pull
 * request execution's Repository Card name. The returned
 * input carries the selection actually shown.
 */
export async function loadExecution(input: ExecutionInput, context: Context): Promise<VerificationExecutionProps> {
  const one = async (request: WireRequest) => (await context.executor.run([request], context.signal))[0] as WireResult;
  const id = input.executionId;
  const status = await one({ op: 'verification.get_execution', execution_id: id });
  if (!status.ok) return { input, result: failed(status.problem) };
  const state = status.value as VerificationExecutionState;
  // A judged execution opens on its first failed requirement until the reader selects something, or nothing.
  const first = input.node === undefined ? state.result?.outcome.requirements.find((each) => each.outcome === 'failed') : undefined;
  if (first) input = { ...input, node: `req:${first.id}` };
  const [type, node] = (input.node ?? '').split(/:(.*)/s);
  const slot = type === 'input' ? state.group?.sources[node] : undefined;
  const drifted = type === 'req' && state.result?.outcome.requirements.some((each) => each.id === node && each.detail?.drift);

  const absent = (read: WireResult) => !read.ok && read.problem.code === NOT_QUEUED;
  const defined = readDefinition(state, one);
  const [definition, invocations, publications, details, selected, members, inputs, drift, repository] = await Promise.all([
    defined,
    // Step states come from recorded progress, and so do requirement states until a result is recorded. Without a readable definition, recorded progress is the only list of steps to select.
    defined.then(async (definition) => {
      if (state.result && type !== 'step' && definition.state === 'populated' && !definition.data.graph.steps.length) return undefined;
      const read = await one({ op: 'verification.list_invocations', execution_id: id, limit: EXECUTION_PAGE, ...(input.invocations ? { cursor: input.invocations } : {}) });
      return absent(read) ? undefined : part<CardTypes.InvocationPage>(read);
    }),
    type === 'outputs' || type === 'step'
      ? one({ op: 'verification.list_publications', execution_id: id, limit: EXECUTION_PAGE, ...(input.publications ? { cursor: input.publications } : {}) }).then(
          (read): Part<CardTypes.PublicationPage> => (absent(read) ? { state: 'empty', reason: 'This execution ran directly and retains no outputs.' } : part<CardTypes.PublicationPage>(read))
        )
      : undefined,
    !input.details
      ? undefined
      : !state.result
        ? ({ state: 'empty', reason: 'Details are recorded when the execution completes with a visible result.' } as const)
        : one({ op: 'verification.get_execution_details', execution_id: id }).then((read) => part<CardTypes.VerificationDetails>(read)),
    type === 'step' && input.producer && input.output ? readPublication({ execution_id: id, step_id: node, name: input.output, ...input.producer }, one) : undefined,
    slot
      ? context.executor
          .run(slot.execution_ids.slice(0, MEMBER_READS).map((member) => ({ op: 'verification.get_execution', execution_id: member }) as const), context.signal)
          .then((reads) => slot.execution_ids.map((member, index): SessionMember => ({ execution_id: member, ...(reads[index] ? { status: part<VerificationExecutionState>(reads[index]) } : {}) })))
      : undefined,
    type === 'input' && !slot
      ? one({ op: 'bifrost.query', sql: INPUTS, params: [id], max_rows: 1 }).then(
          (read): Part<unknown> =>
            !read.ok
              ? failed(read.problem)
              : (read.value as QueryResult).rows[0]
                ? { state: 'populated', data: (json((read.value as QueryResult).rows[0][0]) as Record<string, unknown> | null)?.[node] }
                : { state: 'empty', reason: 'The accepted inputs of this execution are not yet visible. Refresh to read them again.' }
        )
      : undefined,
    drifted ? readDrift(id, node, one) : undefined,
    // A pull request's status names its provider repository; the Card name its review sets are read by is on the retained input.
    state.pull_request ? one({ op: 'bifrost.query', sql: REPOSITORY, params: [id], max_rows: 1 }).then((read) => (read.ok ? text((read.value as QueryResult).rows[0]?.[0]) : null)) : undefined
  ]);
  return {
    input,
    result: {
      state: 'populated',
      data: { status: state, definition, ...(invocations && { invocations }), ...(publications && { publications }), ...(details && { details }), ...selected, ...(members && { members }), ...(inputs && { inputs }), ...(drift && { drift }), ...(repository && { repository }) }
    }
  };
}

/**
 * A recorded state as its node shows it: the `EXECUTION_MARKS` value its glyph,
 * word and tone come from, the words that replace that word when items differ,
 * and whether it recedes.
 */
export interface NodeState {
  readonly value: string;
  readonly word?: string;
  readonly dim?: boolean;
}

type Invocation = CardTypes.ExecutionInvocation;
/** The identity of one invocation without its attempt: its step, item and iteration. */
const slotKey = (producer: CardTypes.Producer) => `${producer.item_index ?? ''}/${producer.iteration_index ?? ''}`;
/** Whether two producers name the same step, item, iteration and attempt. */
export const sameProducer = (a: CardTypes.Producer, b: Omit<CardTypes.Producer, 'step_id'> & { step_id?: string }) =>
  (b.step_id === undefined || a.step_id === b.step_id) && a.attempt === b.attempt && (a.item_index ?? null) === (b.item_index ?? null) && (a.iteration_index ?? null) === (b.iteration_index ?? null);

/** The last record of each invocation attempt of `step`, in first-seen order. */
export function attemptsOf(items: readonly Invocation[], step: string): Invocation[] {
  const last = new Map<string, Invocation>();
  for (const item of items) if (item.producer.step_id === step) last.set(`${slotKey(item.producer)}/${item.producer.attempt}`, item);
  return [...last.values()];
}

/** One state for several recorded values: the value they share, else a failure, counted, when any failed. */
function together(values: readonly string[]): Pick<NodeState, 'value' | 'word'> {
  if (new Set(values).size === 1) return { value: values[0] };
  const failed = values.filter((value) => value === 'failed').length;
  return { value: failed ? 'failed' : values[0], word: `${failed} of ${values.length} failed` };
}

/**
 * The recorded state of every step and requirement node, by node key. A step
 * reads the last attempt of each of its items and iterations; a requirement
 * reads the compact result, else its recorded requirement attempts. Nothing
 * here positions or sizes a node.
 */
export function executionStates(graph: DefinitionGraph, status: VerificationExecutionState, items: readonly Invocation[]): Record<string, NodeState> {
  const done = settled(status.status);
  const idle: NodeState = { value: done ? 'not run' : 'not started', dim: done };
  const states: Record<string, NodeState> = {};
  for (const step of graph.steps) {
    const latest = new Map<string, Invocation>();
    for (const item of attemptsOf(items, step.id)) if ((latest.get(slotKey(item.producer))?.producer.attempt ?? 0) <= item.producer.attempt) latest.set(slotKey(item.producer), item);
    const last = [...latest.values()];
    const speculation = last.find((item) => item.speculation !== 'none')?.speculation;
    const dim = speculation === 'discarded' || speculation === 'blocked';
    if (last.length === 0) states[step.key] = idle;
    else if (last.some((item) => item.state === 'started')) states[step.key] = done ? { value: 'not finished', dim: true } : { value: 'running', dim };
    else {
      const recorded = last.map((item) => (item.state === 'action' ? item.status : item.state === 'requirement' ? item.outcome : 'started'));
      states[step.key] = { ...together(recorded), dim };
    }
  }
  for (const requirement of [...graph.steps.flatMap((step) => step.requirements), ...graph.requirements]) states[requirement.key] = requirementState(status, items, requirement.id);
  return states;
}

/**
 * The recorded state of one requirement, with or without its definition: its
 * judgments in the selected result, else its settled requirement invocations,
 * else not judged.
 */
export function requirementState(status: VerificationExecutionState, items: readonly Invocation[], id: string): NodeState {
  const judged = status.result?.outcome.requirements.filter((each) => each.id === id).map((each) => each.outcome);
  const recorded = judged?.length ? judged : requirementAttempts(items, id).map((item) => item.outcome);
  return recorded.length ? together(recorded) : { value: 'not judged', dim: settled(status.status) };
}

/** The settled requirement invocations of `id`, one per item, iteration and attempt, in first-seen order. */
export const requirementAttempts = (items: readonly Invocation[], id: string) => attemptsOf(items, id).flatMap((item) => (item.state === 'requirement' ? [item] : []));

/** A refused part's problem, for callers that only report it. */
export const refusal = (part: Part<unknown> | undefined): Problem | undefined => (part?.state === 'denied' || part?.state === 'error' ? part.problem : undefined);
