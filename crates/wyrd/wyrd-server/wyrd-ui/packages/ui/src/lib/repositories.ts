/**
 * Repository loaders: Repository Cards joined to the tenant's GitHub
 * connection by provider `repository_id`, for display only. Registration,
 * GitHub access and automatic verification stay three separate facts:
 * selection and reachability are the Card's server-derived
 * `status.repository`, and the connection only says why access is missing.
 */
import type { CardTypes } from 'wyrd-ai';
import {
  failed,
  wyrdProblem,
  type CardRef,
  type CardSummary,
  type Executor,
  type GithubConnectionView,
  type Loaded,
  type Problem,
  type RegisteredCard,
  type WireRequest
} from './contract.js';
import { compareVersions, listVersions, type CardVersions } from './cards.js';
import { fmtYaml } from './format.js';
import type { GithubIntegrationInput, RepositoryDeclareInput, RepositoryIndexInput, RepositoryPolicyInput } from './schemas.js';
import { runsWhen } from './verifier.js';

interface Context {
  readonly executor: Executor;
  readonly signal?: AbortSignal;
}
type Read<T> = Extract<Loaded<T>, { state: 'populated' | 'denied' | 'error' }>;

/** Whether the tenant's connection reaches a repository, and when not, why; `unreachable` when the reason cannot be read. */
export type RepositoryAccess = 'granted' | 'removed' | 'lost' | 'none' | 'unreachable';
/** Whether pull requests start verification: no Card, no selected version, no access, no binding, or ready. */
export type RepositoryReadiness = 'ready' | 'inactive' | 'none' | 'unselected' | 'undeclared';

/** How a fact is drawn: a glyph and a word, toned by token. */
interface FactWord {
  readonly glyph: string;
  readonly tone: 'ok' | 'warn' | 'danger' | 'muted';
  readonly word: string;
}
export const ACCESS_WORDS: Readonly<Record<RepositoryAccess, FactWord>> = {
  granted: { glyph: '✓', tone: 'ok', word: 'Granted' },
  removed: { glyph: '✕', tone: 'danger', word: 'Removed' },
  lost: { glyph: '✕', tone: 'danger', word: 'Lost' },
  none: { glyph: '○', tone: 'muted', word: 'No connection' },
  unreachable: { glyph: '✕', tone: 'danger', word: 'No access' }
};
export const READINESS_WORDS: Readonly<Record<RepositoryReadiness, FactWord>> = {
  ready: { glyph: '●', tone: 'ok', word: 'Ready' },
  inactive: { glyph: '⊘', tone: 'warn', word: 'Inactive' },
  none: { glyph: '○', tone: 'muted', word: 'Not configured' },
  unselected: { glyph: '○', tone: 'muted', word: 'Not selected' },
  undeclared: { glyph: '○', tone: 'muted', word: 'Not declared' }
};

/** One sentence saying why access reads as it does. */
export function accessWhy(access: RepositoryAccess, fullName: string | undefined, installation?: number | null): string {
  const name = fullName ?? 'This repository';
  return {
    granted: `${installation ? `Installation ${installation}` : 'The GitHub installation'} can read ${fullName ?? 'this repository'}.`,
    removed: `${name} is no longer selected for the installation on GitHub.`,
    lost: 'The GitHub installation is suspended or removed.',
    none: 'GitHub is not connected for this tenant.',
    unreachable: "This tenant's GitHub connection does not reach the repository."
  }[access];
}

/** One sentence saying why automatic verification reads as it does. */
export function readinessWhy(readiness: RepositoryReadiness, bindings: number, selected?: string): string {
  return {
    ready: `${bindings} Verifier${bindings === 1 ? '' : 's'} run on pull_request.`,
    inactive: `No new executions start without GitHub access. ${bindings} binding${bindings === 1 ? '' : 's'} stay declared.`,
    none: `Selected version ${selected} has verifier = [].`,
    unselected: 'No declaration of this Repository has completed, so no version is selected.',
    undeclared: 'No Repository Card is registered for this repository.'
  }[readiness];
}

/** Access from the Card's server-derived `full_name`; the connection only explains its absence. */
function accessOf(fullName: string | null | undefined, connection: Read<GithubConnectionView>): RepositoryAccess {
  if (fullName) return 'granted';
  if (connection.state !== 'populated') return 'unreachable';
  return ({ not_connected: 'none', suspended: 'lost', active: 'removed' } as const)[connection.data.state];
}

function readinessOf(access: RepositoryAccess, selected: string | undefined, bindings: number): RepositoryReadiness {
  if (selected === undefined) return 'unselected';
  if (access !== 'granted') return 'inactive';
  return bindings === 0 ? 'none' : 'ready';
}

/** One repository: a Repository Card family, a reachable GitHub repository with no Card, or a Card whose read was refused. */
export interface RepositoryRow {
  readonly repositoryId?: string;
  readonly fullName?: string;
  /** The Card family: its name, version count, selected version, and the version the row opens. */
  readonly card?: { readonly name: string; readonly versions: number; readonly selected?: string; readonly opens: string };
  /** Bindings of the selected version. */
  readonly bindings: number;
  readonly access?: RepositoryAccess;
  readonly readiness?: RepositoryReadiness;
  /** Why this Card could not be read; its access and readiness are then unknown. */
  readonly problem?: Problem;
}

/** The joined rows of one tenant. */
export interface RepositoryJoin {
  readonly rows: readonly RepositoryRow[];
  /** Why the rows may be incomplete; repositories without a Card are then not claimed. */
  readonly partial?: string;
}

const LIST_PAGE = 100;
const LIST_PAGES = 5;
/** The most Repository Card families one join reads. */
const FAMILY_READS = 50;

const repositoryOf = (card: RegisteredCard) => (card.status as { repository?: CardTypes.RepositoryStatus } | undefined)?.repository;
const specOf = (card: RegisteredCard) => card.spec as CardTypes.RepositorySpec;

async function readConnection(context: Context): Promise<Read<GithubConnectionView>> {
  const [connection] = await context.executor.run([{ op: 'github.get_connection' }], context.signal);
  return connection.ok ? { state: 'populated', data: connection.value } : failed(connection.problem);
}

/**
 * Every Repository Card family with its selected policy, then each reachable
 * GitHub repository no Card names. A refused listing is the result; a refused
 * Card keeps its row.
 */
// ponytail: reads at most 500 versions and 50 families, two Card reads each; past that the rows say they are partial. A server-side join replaces this if tenants outgrow it.
async function joinRepositories(connection: Read<GithubConnectionView>, context: Context): Promise<Read<RepositoryJoin>> {
  const listed: CardSummary[] = [];
  let cursor: string | null = null;
  for (let page = 0; page < LIST_PAGES; page += 1) {
    context.signal?.throwIfAborted();
    const request: Extract<WireRequest, { op: 'cards.list' }> = { op: 'cards.list', kind: 'Repository', include_prerelease: true, limit: LIST_PAGE, cursor };
    const [answer] = await context.executor.run([request], context.signal);
    if (!answer.ok) return failed(answer.problem);
    listed.push(...answer.value.items);
    cursor = answer.value.next_cursor;
    if (cursor === null) break;
  }
  const families = new Map<string, string[]>();
  for (const each of listed) families.set(each.name, [...(families.get(each.name) ?? []), each.version]);
  const names = [...families.keys()].sort().slice(0, FAMILY_READS);
  const ref = (name: string, version: string): CardRef => ({ kind: 'Repository', name, version });
  const newest = names.map((name) => families.get(name)!.sort(compareVersions)[0]);
  const read = (refs: readonly CardRef[]) => (refs.length ? context.executor.run(refs.map((card_ref) => ({ op: 'cards.get_by_ref', card_ref }) as const), context.signal) : Promise.resolve([]));
  const first = await read(names.map((name, index) => ref(name, newest[index])));
  // The selected version is read too when it is not the newest one.
  const wanted = first.flatMap((answer, index) => {
    const selected = answer.ok ? repositoryOf(answer.value)?.selected_version : undefined;
    return selected && selected !== newest[index] ? [{ index, ref: ref(names[index], selected) }] : [];
  });
  const second = await read(wanted.map((each) => each.ref));
  const rows = names.map((name, index): RepositoryRow => {
    const opens = wanted.find((each) => each.index === index);
    const answer = opens ? second[wanted.indexOf(opens)] : first[index];
    const card = { name, versions: families.get(name)!.length, opens: opens?.ref.version ?? newest[index] };
    if (!answer.ok) return { card, bindings: 0, problem: answer.problem };
    const status = repositoryOf(answer.value);
    const selected = status?.selected_version ?? undefined;
    const bindings = selected === undefined ? 0 : (specOf(answer.value).verified_by?.length ?? 0);
    const access = accessOf(status?.full_name, connection);
    return {
      repositoryId: specOf(answer.value).repository_id ?? undefined,
      ...(status?.full_name ? { fullName: status.full_name } : {}),
      card: { ...card, ...(selected ? { selected } : {}) },
      bindings,
      access,
      readiness: readinessOf(access, selected, bindings)
    };
  });
  const partial =
    cursor !== null || families.size > names.length
      ? `Showing the first ${names.length} Repository Cards.`
      : rows.some((row) => row.problem)
        ? 'A Repository Card could not be read, so GitHub repositories without a Card are not listed.'
        : undefined;
  const known = new Set(rows.map((row) => row.repositoryId));
  const undeclared =
    partial || connection.state !== 'populated'
      ? []
      : connection.data.repositories
          .filter((each) => !known.has(each.repository_id))
          .map((each): RepositoryRow => ({ repositoryId: each.repository_id, fullName: each.full_name, bindings: 0, access: 'granted', readiness: 'undeclared' }))
          .sort((a, b) => a.fullName!.localeCompare(b.fullName!));
  return { state: 'populated', data: { rows: [...rows, ...undeclared], ...(partial ? { partial } : {}) } };
}

/** Props of `RepositoryIndex`. */
export interface RepositoryIndexProps {
  readonly input: RepositoryIndexInput;
  readonly result: Loaded<{
    /** The rows the search and Show choice keep, of `total` joined. */
    readonly rows: readonly RepositoryRow[];
    readonly total: number;
    readonly partial?: string;
    /** The connection read on its own: a refusal never hides the Cards. */
    readonly connection: Read<GithubConnectionView>;
  }>;
}

/** Load the Repositories index: the bounded join, narrowed by the search text and the Show choice. */
export async function loadRepositoryIndex(input: RepositoryIndexInput, context: Context): Promise<RepositoryIndexProps> {
  const connection = await readConnection(context);
  const join = await joinRepositories(connection, context);
  if (join.state !== 'populated') return { input, result: join };
  const term = input.query?.trim().toLowerCase();
  const shows = (row: RepositoryRow) =>
    input.show === undefined || input.show === 'all' || (input.show === 'inactive' ? ['inactive', 'none', 'unselected'].includes(row.readiness ?? '') : row.readiness === input.show);
  const rows = join.data.rows.filter((row) => shows(row) && (!term || `${row.card?.name ?? ''} ${row.fullName ?? ''}`.toLowerCase().includes(term)));
  return { input, result: { state: 'populated', data: { rows, total: join.data.rows.length, ...(join.data.partial ? { partial: join.data.partial } : {}), connection } } };
}

/** One declared binding of the viewed version. */
export interface RepositoryBinding {
  /** The Verifier's name, or the authored reference when it is not an exact Card. */
  readonly name: string;
  readonly version?: string;
  /** The exact Verifier Card, when the binding pins one. */
  readonly ref?: CardRef;
  readonly startsOn: string;
  /** Whether this binding starts executions: only the selected version's bindings can. */
  readonly state: 'active' | 'inactive · no access' | 'not selected';
}

/** Props of `RepositoryPolicy`. */
export interface RepositoryPolicyProps {
  readonly input: RepositoryPolicyInput;
  readonly result: Loaded<{
    readonly card: RegisteredCard;
    readonly versions: Read<CardVersions>;
    /** Whether the viewed version is the family's selected policy, and which version is. */
    readonly selected: boolean;
    readonly selectedVersion?: string;
    readonly fullName?: string;
    readonly installation?: number | null;
    readonly access: RepositoryAccess;
    /** Readiness of the selected policy; absent when that version could not be read. */
    readonly readiness?: RepositoryReadiness;
    /** Bindings of the selected policy. */
    readonly selectedBindings: number;
    readonly bindings: readonly RepositoryBinding[];
    /** The shown declaration text: the returned Card as YAML, or its `wyrd.toml` projection. */
    readonly declaration: string;
  }>;
}

const exact = (verifier: CardTypes.VerificationBinding['verifier']): CardRef | undefined =>
  typeof verifier === 'object' && 'version' in verifier && typeof verifier.version === 'string' ? { kind: 'Verifier', name: verifier.name, version: verifier.version } : undefined;

/** The `wyrd.toml` a declaration of these verifier entries is written as. */
function toml(lead: readonly string[], verifiers: readonly string[]): string {
  return [...lead, '[repository.verified_by]', ...(verifiers.length ? ['verifier = [', ...verifiers.map((each) => `  ${each},`), ']'] : ['verifier = []']), 'runs_on = { kind = "pull_request" }'].join('\n');
}

/** A Card's bindings as `wyrd.toml`, when every one pins an exact Verifier on pull requests; the file's own text is not retained. */
function projectToml(card: RegisteredCard): string {
  const bindings = specOf(card).verified_by ?? [];
  const identity = `Repository/${card.metadata.name}@${card.metadata.version}`;
  const refs = bindings.map((binding) => (runsWhen(binding.runs_on) === 'runs_on: pull_request' ? exact(binding.verifier) : undefined));
  if (refs.some((each) => each === undefined)) return `# ${identity} has a binding wyrd.toml cannot express. Read the Card YAML.`;
  return toml(
    [`# Projection of ${identity}, not the declared file.`],
    refs.map((each) => `{ kind = "Verifier", name = ${JSON.stringify(each!.name)}, version = ${JSON.stringify(each!.version)} }`)
  );
}

/**
 * Load one exact Repository Card version: its bindings, whether it is the
 * selected policy, and access. A refused Card issues no further read; a
 * version that is not selected reads the selected one for readiness only.
 */
export async function loadRepositoryPolicy(input: RepositoryPolicyInput, context: Context): Promise<RepositoryPolicyProps> {
  const [read] = await context.executor.run([{ op: 'cards.get_by_ref', card_ref: input.cardRef }], context.signal);
  if (!read.ok) return { input, result: failed(read.problem) };
  const card = read.value;
  if (card.kind !== 'Repository')
    return { input, result: { state: 'error', problem: wyrdProblem('validation', `${input.cardRef.name}@${input.cardRef.version} is a ${card.kind} Card, not a Repository`) } };
  const status = repositoryOf(card);
  const selectedVersion = status?.selected_version ?? undefined;
  const selected = status?.selected === true;
  const [versions, connection, [other]] = await Promise.all([
    listVersions(card, context),
    readConnection(context),
    selectedVersion && !selected ? context.executor.run([{ op: 'cards.get_by_ref', card_ref: { ...input.cardRef, version: selectedVersion } }], context.signal) : Promise.resolve([undefined])
  ]);
  const access = accessOf(status?.full_name, connection);
  const policy = selected ? card : other?.ok ? other.value : undefined;
  const selectedBindings = policy ? (specOf(policy).verified_by?.length ?? 0) : 0;
  const readiness = selectedVersion === undefined ? 'unselected' : policy ? readinessOf(access, selectedVersion, selectedBindings) : undefined;
  const state = !selected ? 'not selected' : access === 'granted' ? 'active' : 'inactive · no access';
  const bindings = (specOf(card).verified_by ?? []).map((binding): RepositoryBinding => {
    const ref = exact(binding.verifier);
    return { name: ref?.name ?? (typeof binding.verifier === 'string' ? binding.verifier : 'sibling Verifier'), ...(ref ? { version: ref.version, ref } : {}), startsOn: runsWhen(binding.runs_on), state };
  });
  return {
    input,
    result: {
      state: 'populated',
      data: {
        card,
        versions,
        selected,
        ...(selectedVersion ? { selectedVersion } : {}),
        ...(status?.full_name ? { fullName: status.full_name } : {}),
        ...(connection.state === 'populated' ? { installation: connection.data.installation_id } : {}),
        access,
        ...(readiness ? { readiness } : {}),
        selectedBindings,
        bindings,
        declaration: input.declaration === 'toml' ? projectToml(card) : fmtYaml(card)
      }
    }
  };
}

/** Props of `RepositoryDeclare`. */
export interface RepositoryDeclareProps {
  readonly input: RepositoryDeclareInput;
  readonly result: Loaded<{ readonly repositoryId: string; readonly fullName: string; readonly host: string; readonly installation?: number | null; readonly toml: string }>;
}

/**
 * Load one reachable GitHub repository that has no Repository Card. An id the
 * connection does not reach, or one a Card already names, is not found and
 * names no repository; a refused connection issues no Card read.
 */
export async function loadRepositoryDeclare(input: RepositoryDeclareInput, context: Context): Promise<RepositoryDeclareProps> {
  const connection = await readConnection(context);
  if (connection.state !== 'populated') return { input, result: connection };
  const join = await joinRepositories(connection, context);
  if (join.state !== 'populated') return { input, result: join };
  const row = join.data.rows.find((each) => !each.card && each.repositoryId === input.repositoryId);
  if (!row?.fullName) return { input, result: { state: 'error', problem: wyrdProblem('notFound', 'No reachable GitHub repository without a Repository Card has this id') } };
  return {
    input,
    result: {
      state: 'populated',
      data: {
        repositoryId: input.repositoryId,
        fullName: row.fullName,
        host: connection.data.host,
        installation: connection.data.installation_id,
        toml: toml(['# wyrd.toml at the repository root. provider, host and repository_id', '# are inferred from origin and verified by the server.'], ['"./review/pr-review.yaml"'])
      }
    }
  };
}

/** Props of `GithubIntegration`. */
export interface GithubIntegrationProps {
  readonly input: GithubIntegrationInput;
  readonly result: Loaded<{
    readonly connection: GithubConnectionView;
    /** Repository Cards joined to the reachable repositories; a refusal leaves registration unclaimed. */
    readonly cards: Read<RepositoryJoin>;
  }>;
}

/** Load the tenant's GitHub connection, then the Cards its repositories are joined to. A refused or unconfigured connection issues no Card read. */
export async function loadGithubIntegration(input: GithubIntegrationInput, context: Context): Promise<GithubIntegrationProps> {
  const connection = await readConnection(context);
  if (connection.state !== 'populated') return { input, result: connection };
  const join = await joinRepositories(connection, context);
  return { input, result: { state: 'populated', data: { connection: connection.data, cards: join } } };
}
