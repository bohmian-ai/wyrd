/**
 * Portable loaders: plan reads, run them through the supplied executor, and
 * shape responses into component props. Pure TypeScript; no Node or browser
 * API and no credential, so the same loader runs on every host.
 */
import {
  failed,
  wyrdProblem,
  type ActionIntent,
  type CardSummary,
  type Executor,
  type Loaded,
  type Problem,
  type QueryParam,
  type QueryResult,
  type RegisteredCard,
  type WireRequest
} from './contract.js';
import type { ZodType } from 'zod';
import { columnIndex } from './plot.js';
import {
  components,
  type AbsoluteRange,
  type CardHeaderInput,
  type ChartInput,
  type ComponentName,
  resolveRange
} from './schemas.js';

export * from './contract.js';
export * from './schemas.js';

/** What every loader receives from its host. */
export interface LoadContext {
  readonly executor: Executor;
  /** The view's range, already resolved once for the whole load; a panel's own range wins over it. */
  readonly range?: AbsoluteRange;
  /** Aborted when the panel's input or range changes, it is removed, or its host leaves. */
  readonly signal?: AbortSignal;
}

/** Props of `Chart`. */
export interface ChartProps {
  readonly input: ChartInput;
  /** The exact SQL and binds that ran, for Open in Query; absent before planning. */
  readonly executed?: { readonly sql: string; readonly params: readonly QueryParam[] };
  readonly result: Loaded<QueryResult>;
}

/** The exact versions of one Card, newest first. */
export interface CardVersions {
  readonly items: readonly CardSummary[];
  /** More versions exist than the loader pages through. */
  readonly truncated: boolean;
}

/** Props of `CardHeader`. */
export interface CardHeaderProps {
  readonly input: CardHeaderInput;
  readonly result: Loaded<{ readonly card: RegisteredCard; readonly versions: Loaded<CardVersions> }>;
}

/** Props of each catalog component. */
export interface ComponentProps {
  Chart: ChartProps;
  CardHeader: CardHeaderProps;
}

/** Bucket intervals a chart may bind as `$3`, finest first. */
const BUCKETS: readonly [string, number][] = [
  ['1 minute', 60_000],
  ['5 minutes', 300_000],
  ['15 minutes', 900_000],
  ['1 hour', 3_600_000],
  ['6 hours', 21_600_000],
  ['1 day', 86_400_000]
];

/** The finest bucket that keeps `range` at or under 200 points. */
export function bucketFor(range: AbsoluteRange): string {
  const span = Date.parse(range.to) - Date.parse(range.from);
  return (BUCKETS.find(([, width]) => span / width <= 200) ?? BUCKETS[BUCKETS.length - 1])[0];
}

/** An identifier in the query owner's SQL dialect; a `$` inside one is part of its name. */
const IDENTIFIER = /[\p{L}_#@][\p{L}0-9_@$#]*/uy;
/** A `$` and the word after it: a placeholder, or the opening tag of a dollar-quoted string. */
const DOLLAR = /\$([\p{L}\p{N}_]*)/uy;

/**
 * The highest `$1`..`$3` placeholder `sql` references, read the way the query
 * owner's SQL dialect tokenizes it. Quoted strings (doubled quotes, and
 * backslash escapes in `E'...'`), quoted identifiers, dollar-quoted strings,
 * and line or nested block comments hold no placeholders.
 */
function highestBind(sql: string): number {
  let highest = 0;
  let at = 0;
  const close = (quote: string, backslash: boolean): number => {
    let next = at + 1;
    while (next < sql.length) {
      if (backslash && sql[next] === '\\') next += 2;
      else if (sql[next] !== quote) next += 1;
      else if (sql[next + 1] === quote) next += 2;
      else return next + 1;
    }
    return next;
  };
  while (at < sql.length) {
    const char = sql[at];
    if ((char === 'e' || char === 'E') && sql[at + 1] === "'") {
      at += 1;
      at = close("'", true);
    } else if (char === "'" || char === '"' || char === '`') {
      at = close(char, false);
    } else if (sql.startsWith('--', at)) {
      const end = sql.indexOf('\n', at);
      at = end < 0 ? sql.length : end + 1;
    } else if (sql.startsWith('/*', at)) {
      let depth = 0;
      do {
        if (sql.startsWith('/*', at)) [depth, at] = [depth + 1, at + 2];
        else if (sql.startsWith('*/', at)) [depth, at] = [depth - 1, at + 2];
        else at += 1;
      } while (depth > 0 && at < sql.length);
    } else if (char === '$') {
      DOLLAR.lastIndex = at;
      const [token, tag] = DOLLAR.exec(sql) ?? ['$', ''];
      at += token.length;
      if (sql[at] === '$') {
        const end = sql.indexOf(`$${tag}$`, at + 1);
        at = end < 0 ? sql.length : end + tag.length + 2;
      } else if (/^[1-3]$/.test(tag)) {
        highest = Math.max(highest, Number(tag));
      }
    } else {
      IDENTIFIER.lastIndex = at;
      at += IDENTIFIER.exec(sql)?.[0].length || 1;
    }
  }
  return highest;
}

/**
 * The binds an authored chart SQL references: `$1`/`$2` are the range bounds
 * and `$3` the bucket. Only the highest referenced position is bound; the SQL
 * itself is never rewritten.
 */
export function chartParams(sql: string, range: AbsoluteRange | undefined): QueryParam[] | Problem {
  const used = highestBind(sql);
  if (used === 0) return [];
  if (range === undefined) return wyrdProblem('validation', 'The SQL binds $1..$3 but no range was supplied.');
  return [range.from, range.to, bucketFor(range)].slice(0, used);
}

/** Arrow types a chart may plot as a number. */
const NUMERIC = /^(U?Int\d+|Float\d+|Decimal)/;
/** Arrow types a time-series `x` may hold. */
const TEMPORAL = /^(Timestamp|Date)/;

/** A result that does not match its chart definition. */
function shapeError(detail: string): Loaded<never> {
  return { state: 'error', problem: wyrdProblem('validation', detail) };
}

/**
 * Check `result` against the chart type's shape rules (REQ-025); a mismatch is
 * a panel error, never a blank chart. A duplicated name is checked at the
 * column the plot draws: the first with that name.
 */
export function shapeChart(input: ChartInput, result: QueryResult): Loaded<QueryResult> {
  const need = (name: string, kind: RegExp, role: string): string | undefined => {
    const type = result.columns[columnIndex(result, name)]?.data_type;
    if (type === undefined) return `${role} column "${name}" is not in the result`;
    if (!kind.test(type)) return `${role} column "${name}" is ${type}`;
    return undefined;
  };
  const problems: (string | undefined)[] = [];
  if (input.type === 'timeseries') {
    problems.push(need(input.x, TEMPORAL, 'x'));
    for (const y of input.y) problems.push(need(y, NUMERIC, 'y'));
    if (input.series !== undefined) problems.push(need(input.series, /^(Utf8|LargeUtf8|Utf8View)$/, 'series'));
  } else if (input.type === 'bar') {
    problems.push(need(input.x, /^(Utf8|LargeUtf8|Utf8View)$/, 'x'));
    for (const y of input.y) problems.push(need(y, NUMERIC, 'y'));
  } else if (input.type === 'stat') {
    const y = input.y?.[0] ?? result.columns[0]?.name;
    if (input.y === undefined && result.columns.length !== 1)
      problems.push(`a stat reads one numeric column; the result has ${result.columns.length}`);
    else if (y !== undefined) problems.push(need(y, NUMERIC, 'y'));
    if (result.rows.length > 1) problems.push(`a stat reads one row; the result has ${result.rows.length}`);
  }
  const problem = problems.find((each) => each !== undefined);
  if (problem !== undefined) return shapeError(problem);
  if (result.rows.length === 0) {
    const bound = 'y' in input && input.y ? input.y.join(', ') : 'any column';
    return { state: 'empty', data: result, reason: `The SQL returned no rows for ${bound} in this range.` };
  }
  return { state: 'populated', data: result };
}

/** Load one chart: bind its own range or else the view's, run the authored SQL unchanged, check its shape. */
export async function loadChart(input: ChartInput, context: LoadContext): Promise<ChartProps> {
  const range = input.range ? resolveRange(input.range) : context.range;
  const params = chartParams(input.sql, range);
  if (!Array.isArray(params)) return { input, result: { state: 'error', problem: params } };
  const executed = { sql: input.sql, params };
  const [read] = await context.executor.run([{ op: 'bifrost.query', sql: input.sql, params }], context.signal);
  return { input, executed, result: read.ok ? shapeChart(input, read.value) : failed(read.problem) };
}

/** Versions per `cards.list` page, and the pages a header follows before reporting truncation. */
const VERSION_PAGE = 100;
const VERSION_PAGES = 5;

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

/**
 * Load one exact Card, then the exact versions sharing its resolved identity.
 *
 * The version listing depends on the Card read's resolved name, so a denied or
 * failed Card read issues no listing. A failed listing keeps the Card visible
 * with its own versions state.
 */
export async function loadCardHeader(input: CardHeaderInput, context: LoadContext): Promise<CardHeaderProps> {
  const [read] = await context.executor.run([{ op: 'cards.get_by_ref', card_ref: input.cardRef }], context.signal);
  if (!read.ok) return { input, result: failed(read.problem) };
  const card = read.value;
  const items: CardSummary[] = [];
  let cursor: string | null = null;
  let versions: Loaded<CardVersions> | undefined;
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
    if (!listed.ok) {
      versions = failed(listed.problem);
      break;
    }
    items.push(...listed.value.items);
    cursor = listed.value.next_cursor;
    if (cursor === null) break;
  }
  versions ??= {
    state: 'populated',
    data: { items: items.sort((a, b) => compareVersions(a.version, b.version)), truncated: cursor !== null }
  };
  return { input, result: { state: 'populated', data: { card, versions } } };
}

/** Validation failure of a panel input, naming the component and the failing field. */
export function inputProblem(component: string, issue: { path: readonly PropertyKey[]; message: string }): Problem {
  const field = issue.path.length === 0 ? 'input' : `input.${issue.path.map(String).join('.')}`;
  return wyrdProblem('validation', `${component} ${field}: ${issue.message}`);
}

/** A component's loader, typed by its validated input. */
const LOADERS: {
  [Name in ComponentName]: (input: never, context: LoadContext) => Promise<ComponentProps[Name]>;
} = { Chart: loadChart, CardHeader: loadCardHeader };

/** A panel whose component is in the catalog and whose input passed its schema. */
export interface ValidPanel {
  readonly component: ComponentName;
  readonly input: unknown;
}

/** Validate an untrusted panel against its catalog schema; a `Problem` names the component and field. */
export function validatePanel(component: string, input: unknown): ValidPanel | Problem {
  if (!Object.hasOwn(components, component)) return inputProblem(component, { path: [], message: 'is not in the catalog' });
  const name = component as ComponentName;
  const parsed = components[name].input.safeParse(input);
  return parsed.success ? { component: name, input: parsed.data } : inputProblem(name, parsed.error.issues[0]);
}

/**
 * Validate an untrusted panel input against its catalog schema and run its
 * loader. Returns a `Problem` for an unknown component or invalid input.
 */
export async function loadPanel(
  component: string,
  input: unknown,
  context: LoadContext
): Promise<ComponentProps[ComponentName] | Problem> {
  const panel = validatePanel(component, input);
  if ('code' in panel) return panel;
  return LOADERS[panel.component](panel.input as never, context);
}

/**
 * Validate an untrusted intent against the action schemas its component
 * declares. Hosts call this before acting: only catalog schemas define what
 * an intent may carry.
 */
export function validateIntent(component: ComponentName, intent: ActionIntent): ActionIntent | Problem {
  const emits: Readonly<Record<string, { readonly input: ZodType }>> = components[component].emits;
  if (!Object.hasOwn(emits, intent.name)) return wyrdProblem('validation', `${component} does not emit ${intent.name}`);
  const parsed = emits[intent.name].input.safeParse(intent.input);
  if (!parsed.success) return inputProblem(`${component} ${intent.name}`, parsed.error.issues[0]);
  return { name: intent.name, input: parsed.data };
}
