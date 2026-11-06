/**
 * Card and Prompt loaders: one exact Card read, or the cards.list inventory,
 * shaped into component props. Pure TypeScript over an executor, like every
 * loader; nothing here re-derives what the server owns (status,
 * relationships, compiled variables).
 */
import type { CardRelationships, ExecutionRoute, PromptSpec } from 'wyrd-ai';
import {
  failed,
  wyrdProblem,
  type CardRef,
  type CardSummary,
  type Executor,
  type Loaded,
  type RegisteredCard,
  type StoredArtifactEntry,
  type WireRequest
} from './contract.js';
import type { CardDetailInput, CardFactsInput, CardInventoryInput, VersionDiffInput } from './schemas.js';

/** What a card loader receives from its host; the same shape as every `LoadContext`. */
interface Context {
  readonly executor: Executor;
  readonly signal?: AbortSignal;
}

/** Order exact versions newest first by semantic version, prereleases below their release. */
export function compareVersions(left: string, right: string): number {
  const parse = (version: string) => {
    const [core, pre] = version.split('+')[0].split(/-(.*)/s);
    return { parts: core.split('.').map(Number), pre };
  };
  const a = parse(left);
  const b = parse(right);
  for (let index = 0; index < 3; index += 1) {
    const difference = (b.parts[index] ?? 0) - (a.parts[index] ?? 0);
    if (difference !== 0) return difference;
  }
  if (a.pre === b.pre) return 0;
  if (a.pre === undefined) return -1;
  if (b.pre === undefined) return 1;
  return b.pre.localeCompare(a.pre, undefined, { numeric: true });
}

/** The exact versions of one Card, newest first. */
export interface CardVersions {
  readonly items: readonly CardSummary[];
  /** More versions exist than the loader pages through. */
  readonly truncated: boolean;
}

/** Versions per `cards.list` page, and the pages a version listing follows before reporting truncation. */
const VERSION_PAGE = 100;
const VERSION_PAGES = 5;

/** Every listed version sharing `card`'s resolved kind and name, prereleases included, newest first. */
export async function listVersions(card: RegisteredCard, context: Context): Promise<Read<CardVersions>> {
  const items: CardSummary[] = [];
  let cursor: string | null = null;
  for (let page = 0; page < VERSION_PAGES; page += 1) {
    context.signal?.throwIfAborted();
    const request: Extract<WireRequest, { op: 'cards.list' }> = {
      op: 'cards.list',
      kind: card.kind,
      name: card.metadata.name,
      include_prerelease: true,
      limit: VERSION_PAGE,
      cursor
    };
    const [listed] = await context.executor.run([request], context.signal);
    if (!listed.ok) return failed(listed.problem);
    items.push(...listed.value.items);
    cursor = listed.value.next_cursor;
    if (cursor === null) break;
  }
  return { state: 'populated', data: { items: items.sort((a, b) => compareVersions(a.version, b.version)), truncated: cursor !== null } };
}

/** Inventory rows per `cards.list` page. */
export const INVENTORY_PAGE = 50;

/** Props of `CardInventory`. */
export interface CardInventoryProps {
  readonly input: CardInventoryInput;
  /** Rows in registration order; `more` when the last page returned a cursor; `filtered` when any filter applied. */
  readonly result: Loaded<{ readonly items: readonly CardSummary[]; readonly more: boolean; readonly filtered: boolean }>;
}

/** One word that is not a query keyword: the inventory matches it against Card names. */
const PLAIN = /^[^\s"'=!<>~()[\],]+$/;
const KEYWORDS = new Set(['and', 'or', 'not', 'in', 'exists']);

/** Escape regex metacharacters, then the query language's string escapes. */
function nameMatch(text: string): string {
  const pattern = `(?i)${text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`;
  return `name =~ "${pattern.replace(/["\\]/g, '\\$&')}"`;
}

/** The `cards.list` filter an inventory query sends: a plain word becomes a name match. */
export function inventoryFilter(query: string | undefined): string | undefined {
  const text = query?.trim();
  if (!text) return undefined;
  return PLAIN.test(text) && !KEYWORDS.has(text.toLowerCase()) ? nameMatch(text) : text;
}

/**
 * Load the inventory: follow `next_cursor` for `input.pages` pages, so a
 * reload or Back restores the same rows. A refused page is the panel's state;
 * rows already read are never shown beside a failure.
 */
export async function loadCardInventory(input: CardInventoryInput, context: Context): Promise<CardInventoryProps> {
  const filter = inventoryFilter(input.query);
  const filtered = filter !== undefined || input.kind !== undefined || input.status !== undefined || input.include_prerelease === true;
  const items: CardSummary[] = [];
  let cursor: string | null = null;
  for (let page = 0; page < (input.pages ?? 1); page += 1) {
    context.signal?.throwIfAborted();
    const request: Extract<WireRequest, { op: 'cards.list' }> = {
      op: 'cards.list',
      limit: INVENTORY_PAGE,
      cursor,
      ...(filter ? { filter } : {}),
      ...(input.kind ? { kind: input.kind } : {}),
      ...(input.status ? { status: input.status } : {}),
      ...(input.include_prerelease ? { include_prerelease: true } : {})
    };
    const [listed] = await context.executor.run([request], context.signal);
    if (!listed.ok) return { input, result: failed(listed.problem) };
    items.push(...listed.value.items);
    cursor = listed.value.next_cursor;
    if (cursor === null) break;
  }
  const data = { items, more: cursor !== null, filtered };
  if (items.length > 0) return { input, result: { state: 'populated', data } };
  const reason = filtered ? 'No Card version matches these filters.' : 'This tenant has no registered Cards.';
  return { input, result: { state: 'empty', data, reason } };
}

/** A settled read: its value, or the refusal or failure that replaced it. */
type Read<T> = Extract<Loaded<T>, { state: 'populated' | 'denied' | 'error' }>;

/** Read one exact Card. */
async function readCard(ref: CardRef, context: Context): Promise<Read<RegisteredCard>> {
  const [read] = await context.executor.run([{ op: 'cards.get_by_ref', card_ref: ref }], context.signal);
  return read.ok ? { state: 'populated', data: read.value } : failed(read.problem);
}

/** Props of `CardFacts`. */
export interface CardFactsProps {
  readonly input: CardFactsInput;
  readonly result: Loaded<RegisteredCard>;
}

/** Load an exact Card's facts: the Card as the server returned it. */
export async function loadCardFacts(input: CardFactsInput, context: Context): Promise<CardFactsProps> {
  return { input, result: await readCard(input.cardRef, context) };
}

/** One exact related Card and the alias a parent composition gave it. */
export type CardRelationship = NonNullable<CardRelationships['outbound_refs']>[number];

/** A Card's server-derived relationships, each direction in the server's order. */
export interface CardRelationshipsData {
  readonly outbound: readonly CardRelationship[];
  readonly inbound: readonly CardRelationship[];
}

/** Props of `RelationshipList`. */
export interface RelationshipListProps {
  readonly input: CardDetailInput;
  readonly result: Loaded<CardRelationshipsData>;
}

/** Load an exact Card's relationships as the server derived them. */
export async function loadRelationshipList(input: CardDetailInput, context: Context): Promise<RelationshipListProps> {
  const read = await readCard(input.cardRef, context);
  if (read.state !== 'populated') return { input, result: read };
  const data = { outbound: read.data.relationships?.outbound_refs ?? [], inbound: read.data.relationships?.inbound_refs ?? [] };
  if (data.outbound.length + data.inbound.length > 0) return { input, result: { state: 'populated', data } };
  return { input, result: { state: 'empty', data, reason: 'This Card references no Card and no Card references it.' } };
}

/** Props of `ArtifactTable`. */
export interface ArtifactTableProps {
  readonly input: CardDetailInput;
  readonly result: Loaded<readonly StoredArtifactEntry[]>;
}

/** Load the files registration recorded for an exact Card version. */
export async function loadArtifactTable(input: CardDetailInput, context: Context): Promise<ArtifactTableProps> {
  const [read] = await context.executor.run([{ op: 'cards.list_artifacts', card_ref: input.cardRef }], context.signal);
  if (!read.ok) return { input, result: failed(read.problem) };
  const files = read.value.artifacts;
  if (files.length > 0) return { input, result: { state: 'populated', data: files } };
  return { input, result: { state: 'empty', data: files, reason: 'Registration recorded no files for this version.' } };
}

/** An exact Prompt's authored spec and the server-derived variables. */
async function readPrompt(ref: CardRef, context: Context): Promise<Read<RegisteredCard & { readonly kind: 'Prompt'; readonly spec: PromptSpec }>> {
  const read = await readCard(ref, context);
  if (read.state !== 'populated' || read.data.kind === 'Prompt') return read as never;
  return { state: 'error', problem: wyrdProblem('validation', `${ref.name}@${ref.version} is a ${read.data.kind} Card, not a Prompt`) };
}

/** Props of `PromptMessages`. */
export interface PromptMessagesProps {
  readonly input: { readonly cardRef: CardRef };
  /** The Prompt spec exactly as authored: portable `system`/`messages`, or native `format`/`body`. */
  readonly result: Loaded<PromptSpec>;
}

/** Load an exact Prompt's authored definition. */
export async function loadPromptMessages(input: { readonly cardRef: CardRef }, context: Context): Promise<PromptMessagesProps> {
  const read = await readPrompt(input.cardRef, context);
  return { input, result: read.state === 'populated' ? { state: 'populated', data: read.data.spec } : read };
}

/** One derived Prompt variable. */
export interface PromptVariable {
  readonly name: string;
  readonly kind: 'text' | 'media';
}

/** Props of `PromptVariables`. */
export interface PromptVariablesProps {
  readonly input: { readonly cardRef: CardRef };
  readonly result: Loaded<readonly PromptVariable[]>;
}

/** The server-derived variables of a Prompt Card, text first, each in the compiler's order. */
function variablesOf(card: RegisteredCard): PromptVariable[] {
  const status = card.status?.prompt;
  return [
    ...(status?.variables ?? []).map((name) => ({ name, kind: 'text' as const })),
    ...(status?.media_variables ?? []).map((name) => ({ name, kind: 'media' as const }))
  ];
}

/** Load an exact Prompt's derived variables from its read-only `status.prompt`. */
export async function loadPromptVariables(input: { readonly cardRef: CardRef }, context: Context): Promise<PromptVariablesProps> {
  const read = await readPrompt(input.cardRef, context);
  if (read.state !== 'populated') return { input, result: read };
  const data = variablesOf(read.data);
  if (data.length > 0) return { input, result: { state: 'populated', data } };
  return { input, result: { state: 'empty', data, reason: 'This Prompt renders with no variables.' } };
}

/** One Agent using a Prompt, with what its own read reported. */
export interface PromptUser {
  readonly ref: CardRef;
  /** The Agent's route and the Services referencing it; a refused Agent read keeps the row with its problem. */
  readonly agent: Loaded<{ readonly route: ExecutionRoute; readonly services: readonly CardRef[] }>;
}

/** Props of `UsedBy`. */
export interface UsedByProps {
  readonly input: { readonly cardRef: CardRef };
  readonly result: Loaded<readonly PromptUser[]>;
}

/** Agents a `UsedBy` panel reads at most; further Agents still list without a route. */
const AGENT_READS = 50;

/**
 * Load the Agents whose spec references an exact Prompt, from its inbound
 * relationships, then read each Agent for its route. A refused Prompt read
 * issues no Agent read.
 */
export async function loadUsedBy(input: { readonly cardRef: CardRef }, context: Context): Promise<UsedByProps> {
  const read = await readPrompt(input.cardRef, context);
  if (read.state !== 'populated') return { input, result: read };
  const refs = (read.data.relationships?.inbound_refs ?? []).map((edge) => edge.ref).filter((ref) => ref.kind === 'Agent');
  if (refs.length === 0) return { input, result: { state: 'empty', data: [], reason: 'No registered Agent uses this Prompt version.' } };
  const reads = await context.executor.run(
    refs.slice(0, AGENT_READS).map((ref) => ({ op: 'cards.get_by_ref', card_ref: ref }) as const),
    context.signal
  );
  const data = refs.map((ref, index): PromptUser => {
    const agent = reads[index];
    if (agent === undefined)
      return { ref, agent: { state: 'error', problem: wyrdProblem('validation', `UsedBy reads at most ${AGENT_READS} Agents`) } };
    if (!agent.ok) return { ref, agent: failed(agent.problem) };
    if (agent.value.kind !== 'Agent') return { ref, agent: { state: 'error', problem: wyrdProblem('upstream', `${ref.name} is not an Agent`) } };
    const services = (agent.value.relationships?.inbound_refs ?? []).map((edge) => edge.ref).filter((each) => each.kind === 'Service');
    return { ref, agent: { state: 'populated', data: { route: agent.value.spec.execution_route, services } } };
  });
  return { input, result: { state: 'populated', data } };
}

/** One authored field that differs between two Prompt versions; an absent side is unset. */
export interface PromptChange {
  readonly field: string;
  readonly from?: unknown;
  readonly to?: unknown;
}

/** What `VersionDiff` compares. */
export interface VersionDiffData {
  /** Every listed version, newest first, for choosing the other side. */
  readonly versions: readonly string[];
  /** The other version, or undefined when the Prompt has only one. */
  readonly other?: string;
  /** Changes from `other` to the selected version. */
  readonly changes: readonly PromptChange[];
}

/** Props of `VersionDiff`. */
export interface VersionDiffProps {
  readonly input: VersionDiffInput;
  readonly result: Loaded<VersionDiffData>;
}

/** The authored Prompt fields a comparison walks, in display order; `variables` is the server's derivation. */
function comparable(card: RegisteredCard & { readonly spec: PromptSpec }): [string, unknown][] {
  const spec = card.spec;
  const messages = spec.messages ?? [];
  const status = card.status?.prompt;
  return [
    ['provider', spec.provider],
    ['model', spec.model],
    ['format', spec.format],
    ['system', spec.system],
    ...messages.map((message, index): [string, unknown] => [`messages[${index}]`, message]),
    ['body', spec.body],
    ['response_format', spec.response_format],
    ['settings', spec.settings],
    ['variables', status?.variables],
    ['media_variables', status?.media_variables]
  ];
}

/** The fields that differ between two Prompt reads; message slots present on only one side appear once. */
export function diffPrompts(
  from: RegisteredCard & { readonly spec: PromptSpec },
  to: RegisteredCard & { readonly spec: PromptSpec }
): PromptChange[] {
  const before = new Map(comparable(from));
  const after = new Map(comparable(to));
  const fields = [...new Set([...before.keys(), ...after.keys()])];
  return fields.flatMap((field) => {
    const a = before.get(field) ?? undefined;
    const b = after.get(field) ?? undefined;
    if (JSON.stringify(a) === JSON.stringify(b)) return [];
    return [{ field, ...(a === undefined ? {} : { from: a }), ...(b === undefined ? {} : { to: b }) }];
  });
}

/**
 * Load a comparison: the Prompt, its versions, then the other version (the
 * one listed below it unless `otherVersion` names one). A refused Prompt read
 * issues no further read; a Prompt with one version is empty.
 */
export async function loadVersionDiff(input: VersionDiffInput, context: Context): Promise<VersionDiffProps> {
  const read = await readPrompt(input.cardRef, context);
  if (read.state !== 'populated') return { input, result: read };
  const listed = await listVersions(read.data, context);
  if (listed.state !== 'populated') return { input, result: listed };
  const versions = listed.data.items.map((item) => item.version);
  const others = versions.filter((version) => version !== input.cardRef.version);
  const other = input.otherVersion ?? others.find((version) => compareVersions(input.cardRef.version, version) < 0) ?? others[0];
  if (other === undefined)
    return { input, result: { state: 'empty', data: { versions, changes: [] }, reason: `Only version ${input.cardRef.version} is registered.` } };
  const otherRead = await readPrompt({ kind: 'Prompt', name: read.data.metadata.name, version: other }, context);
  if (otherRead.state !== 'populated') return { input, result: otherRead };
  const changes = diffPrompts(otherRead.data, read.data);
  const data = { versions, other, changes };
  if (changes.length > 0) return { input, result: { state: 'populated', data } };
  return { input, result: { state: 'empty', data, reason: `Versions ${other} and ${input.cardRef.version} are authored identically.` } };
}
