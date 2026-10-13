import { RANGE_MS } from '$lib/features/observe/core/filter-state';
import type {
  CardKind,
  CardStatus,
  ServiceChart,
  ServiceNode,
  ServicePresentation,
  ServiceSignal
} from '$lib/features/cards/core/types';
import type { components } from './generated/wyrd-http';
import type { Analytical } from './auth/browser-sessions';

type Schemas = components['schemas'];
type CardRef = Schemas['CardRef'];
type BindingStatus = Schemas['VerificationBindingStatus'];

/**
 * The live Service workspace projection: the exact Service Card version's
 * declaration, its binding statuses, and two bounded analytical reads —
 * root spans by the OTEL `service.name`/`service.version` resource equal to
 * the Card's name and version, and verification results owned by the exact
 * Service Card version. Every value is read; nothing is fixture or inferred
 * health. An unreadable source renders its own refusal and never zero.
 */

/** Chart buckets per range: a fixed count keeps every range's read bounded. */
export const BUCKETS = 30;
/** Verification signals keep their own 30-day window, independent of the chart range. */
export const SIGNAL_WINDOW_S = 30 * 24 * 3600;

/** Root-span volume, errors (OTLP status code 2) and mean duration per bucket. */
export const TRACES_SQL = `SELECT CAST(floor((to_unixtime(start_time_unix_nano) - $3) / $5) AS BIGINT) AS bucket,
  COUNT(*) AS spans,
  SUM(CASE WHEN status_code = 2 THEN 1 ELSE 0 END) AS errors,
  AVG(duration_nano) AS mean_duration_nano
FROM vala.traces.spans
WHERE service_name = $1 AND service_version = $2 AND parent_span_id IS NULL
  AND start_time_unix_nano >= to_timestamp_seconds($3) AND start_time_unix_nano < to_timestamp_seconds($4)
GROUP BY 1 ORDER BY 1`;

/** Verdict counts and latest end per binding for the exact owner Service version. */
export const RESULTS_SQL = `SELECT binding_id, implementation, verdict, COUNT(*) AS results, MAX(ended_at) AS last_ended
FROM vala.verification.results
WHERE owner_card_uid = $1 AND ended_at >= to_timestamp_seconds($2) AND ended_at < to_timestamp_seconds($3)
GROUP BY binding_id, implementation, verdict`;

/** Canonical result columns each read interprets, by Arrow type-name prefix. */
export const TRACE_COLUMNS = { bucket: 'Int', spans: 'Int', errors: 'Int', mean_duration_nano: 'Float' };
export const RESULT_COLUMNS = {
  binding_id: 'Utf8',
  implementation: 'Utf8',
  verdict: 'Utf8',
  results: 'Int',
  last_ended: 'Timestamp'
};

/** One section's read: its rows, or the stable refusal code it failed with. */
export type Read<T> =
  | { ok: true; rows: T[]; degraded: boolean }
  | { ok: false; denied: boolean; code: string };
export type TraceBucket = { bucket: number; spans: number; errors: number; meanMs: number };
export type ResultRow = {
  bindingId: string;
  implementation: string;
  verdict: string;
  results: number;
  lastEnded: string;
};

/** The aligned chart window: `BUCKETS` steps of `step` seconds ending at `end`. */
export function bucketWindow(range: string, now: Date): { from: number; end: number; step: number } {
  const step = Math.round((RANGE_MS[range] ?? RANGE_MS['1h']) / 1000 / BUCKETS);
  const end = Math.ceil(now.getTime() / 1000 / step) * step;
  return { from: end - BUCKETS * step, end, step };
}

/** Reject a result whose schema lacks a canonical column or carries the wrong type. */
function checkColumns(result: Analytical, expected: Record<string, string>): void {
  for (const [name, prefix] of Object.entries(expected)) {
    const field = result.fields.find((f) => f.name === name);
    if (!field?.type.startsWith(prefix))
      throw Object.assign(new Error(`analytical column ${name} is not ${prefix}`), {
        code: 'WYRD_UI_ANALYTICAL_SCHEMA',
        status: 502
      });
  }
}

/** Map one analytical read and its failure onto a section `Read`. */
export async function readSection<T>(
  run: () => Promise<Analytical>,
  expected: Record<string, string>,
  row: (r: Record<string, unknown>) => T
): Promise<Read<T>> {
  try {
    const result = await run();
    checkColumns(result, expected);
    return { ok: true, rows: result.rows.map(row), degraded: result.outcome === 'degraded' };
  } catch (cause) {
    const { code, status } = (cause ?? {}) as { code?: unknown; status?: unknown };
    return {
      ok: false,
      denied: status === 403,
      code: typeof code === 'string' ? code : 'WYRD_UI_ANALYTICAL_UNAVAILABLE'
    };
  }
}

/** Decode one trace bucket row (Int64 arrives as `bigint`). */
export function traceRow(r: Record<string, unknown>): TraceBucket {
  return {
    bucket: Number(r.bucket),
    spans: Number(r.spans),
    errors: Number(r.errors),
    meanMs: Number(r.mean_duration_nano) / 1e6
  };
}

/** Decode one result row; the Arrow timestamp arrives as epoch milliseconds. */
export function resultRow(r: Record<string, unknown>): ResultRow {
  return {
    bindingId: String(r.binding_id),
    implementation: String(r.implementation),
    verdict: String(r.verdict),
    results: Number(r.results),
    lastEnded: new Date(Number(r.last_ended)).toISOString()
  };
}

/** One declared binding: its subject (a component occurrence or the Service) and Verifier. */
export type DeclaredBinding = {
  alias: string;
  subject: CardRef;
  verifier: CardRef;
  onFailure: CardRef[];
};

/** Everything the projection reads for one Service version. */
export type ServiceReads = {
  card: Schemas['Card'];
  range: string;
  now: Date;
  bindings: { declared: DeclaredBinding; status: BindingStatus | null }[];
  bindingError?: string;
  traces: Read<TraceBucket>;
  results: Read<ResultRow>;
};

/** A registered reference is a `CardRef`; sibling/inline forms exist only before registration. */
const cardRef = (ref: unknown): CardRef | null =>
  ref !== null && typeof ref === 'object' && 'kind' in ref ? (ref as CardRef) : null;

type ServiceSpec = Schemas['ServiceSpec'];

/** The declared components and bindings of a Service spec, registered references only. */
export function declaredBindings(card: Schemas['Card']): {
  components: { alias: string; ref: CardRef }[];
  bindings: DeclaredBinding[];
} {
  const spec = card.spec as ServiceSpec;
  const self: CardRef = {
    kind: 'Service',
    name: card.metadata.name,
    version: card.metadata.version ?? '',
    space: card.metadata.space,
    uid: card.metadata.uid
  };
  const binding = (alias: string, subject: CardRef, b: Schemas['VerificationBinding']) => {
    const verifier = cardRef(b.verifier);
    return verifier
      ? [{ alias, subject, verifier, onFailure: (b.on_failure ?? []).map(cardRef).filter((r) => !!r) }]
      : [];
  };
  const components = (spec.components ?? []).flatMap((c) => {
    const ref = cardRef(c.ref);
    return ref ? [{ alias: c.alias, ref, verified: c.verified_by ?? [] }] : [];
  });
  return {
    components: components.map(({ alias, ref }) => ({ alias, ref })),
    bindings: [
      ...components.flatMap((c) => c.verified.flatMap((b) => binding(c.alias, c.ref, b))),
      ...(spec.verified_by ?? []).flatMap((b) => binding('service', self, b))
    ]
  };
}

const label = (ref: CardRef) => `${ref.kind} ${ref.name}@${ref.version}`;
const cardHref = (ref: CardRef) => (ref.uid ? `/cards/${encodeURIComponent(ref.uid)}` : '');
const clock = (iso: string) => `${iso.slice(0, 16).replace('T', ' ')}Z`;
const verdictTone: Record<string, CardStatus['tone']> = { passed: 'ok', failed: 'danger' };

/** Project every read onto the Service workspace; no branch fabricates a value. */
export function liveService(reads: ServiceReads): ServicePresentation {
  const { card, range, now, traces, results } = reads;
  const { name } = card.metadata;
  const version = card.metadata.version ?? '';
  const { components } = declaredBindings(card);
  const { end, step } = bucketWindow(range, now);
  const scoped = (signal: string, extra = '') =>
    `/observe/${signal}?service=${encodeURIComponent(name)}${extra}&range={range}`;

  // Latest verdict and counts per binding, from the exact owner version's results.
  const byBinding = new Map<string, ResultRow[]>();
  if (results.ok)
    for (const row of results.rows)
      byBinding.set(row.bindingId, [...(byBinding.get(row.bindingId) ?? []), row]);
  const bindings = reads.bindings.map(({ declared, status }) => {
    const rows = status ? (byBinding.get(status.binding_id) ?? []) : [];
    const latest = rows.toSorted((a, b) => b.lastEnded.localeCompare(a.lastEnded))[0];
    return { declared, status, rows, latest };
  });
  const failing = bindings.filter((b) => b.latest?.verdict === 'failed');
  const evidence = bindings.filter((b) => b.latest);
  const phase = card.status?.phase ?? 'unknown';
  const readFailed = [traces, results].some((r) => !r.ok && !r.denied) || !!reads.bindingError;

  const assessment =
    phase === 'failed'
      ? { tone: 'danger' as const, title: '✕ CARD FAILED', sentence: 'This Service version failed registration.' }
      : readFailed
        ? {
            tone: 'warn' as const,
            title: 'ASSESSMENT INCOMPLETE',
            sentence: `An operational read failed (${[traces, results]
              .flatMap((r) => (r.ok ? [] : [r.code]))
              .concat(reads.bindingError ?? [])
              .join(', ')}); nothing below is inferred from it.`
          }
        : failing.length
          ? {
              tone: 'danger' as const,
              title: '▲ VERIFICATION FAILING',
              sentence: `${failing.length} of ${bindings.length} bindings' latest result failed.`
            }
          : !bindings.length
            ? { tone: 'neutral' as const, title: 'NO VERIFICATION DECLARED', sentence: 'This version declares no Verifier bindings.' }
            : !evidence.length
              ? {
                  tone: 'neutral' as const,
                  title: 'NO EVIDENCE YET',
                  sentence: 'No binding has a recorded result in the last 30 days.'
                }
              : {
                  tone: 'ok' as const,
                  title: '✓ VERIFICATION PASSING',
                  sentence: `No binding's latest result failed (${evidence.length} of ${bindings.length} have results).`
                };

  const ready = bindings.filter((b) => b.status?.readiness === 'ready').length;
  const totalSpans = traces.ok ? traces.rows.reduce((n, r) => n + r.spans, 0) : 0;
  const newest = evidence.map((b) => b.latest!).toSorted((a, b) => b.lastEnded.localeCompare(a.lastEnded))[0];
  const refusal = (r: Read<unknown>) => (r.ok ? '' : r.denied ? 'not authorized' : `unavailable · ${r.code}`);

  return {
    now: new Date(end * 1000).toISOString(),
    overview: {
      default: {
        assessment: {
          ...assessment,
          meta: `${name} ${version} · read ${clock(now.toISOString())}`,
          retry: readFailed || undefined
        },
        channels: [
          { label: 'card', value: phase },
          { label: 'bindings', value: reads.bindingError ? `unavailable · ${reads.bindingError}` : `${ready}/${bindings.length} ready` },
          { label: 'telemetry', value: refusal(traces) || `${totalSpans} root spans · ${range}` },
          {
            label: 'latest result',
            value: refusal(results) || (newest ? `${newest.verdict} · ${clock(newest.lastEnded)}` : 'none in 30d')
          }
        ],
        channelNote: 'card state, binding readiness, telemetry and verification are separate reads — none implies another',
        charts: charts(traces, name, version, range, step, end, scoped),
        addChart: { title: '+ ADD CHART', lines: ['Custom Service charts are not configurable yet.'] },
        signals: bindings.map((b) => signal(b, results, scoped)),
        attention: {
          title: 'NEEDS ATTENTION',
          note: `${failing.length} failing`,
          lines: failing.length
            ? failing.slice(1).map((b) => `${b.declared.verifier.name} → ${b.declared.alias} failed`)
            : [evidence.length ? "No binding's latest result failed." : 'No verification evidence yet, so nothing can be judged.'],
          item: failing[0] && {
            text: `${failing[0].declared.verifier.name} → ${failing[0].declared.alias} failed`,
            sub: `latest ${clock(failing[0].latest!.lastEnded)} · subject ${label(failing[0].declared.subject)}`,
            href: observeLink(failing[0].latest!.implementation, failing[0].declared.verifier, scoped),
            label: 'Investigate →'
          }
        },
        context: {
          components: { text: `${components.length} components · ${bindings.length} bindings`, href: 'composition' },
          activity: results.ok
            ? `${results.rows.reduce((n, r) => n + r.results, 0)} verification results in 30 days`
            : `verification results ${refusal(results)}`,
          investigations: ['logs', 'metrics', 'traces', 'evaluations', 'drift'].map((s) => ({
            label: s[0].toUpperCase() + s.slice(1),
            href: scoped(s)
          })),
          foot: `links carry service=${name} and the visible range`
        }
      },
      variants: {}
    },
    composition: composition(card, bindings, components),
    definition: definition(card, components, declaredBindings(card).bindings)
  };
}

/** The three operating charts over aligned buckets, or each one's explicit refusal. */
function charts(
  traces: Read<TraceBucket>,
  name: string,
  version: string,
  range: string,
  step: number,
  end: number,
  scoped: (s: string, extra?: string) => string
): ServiceChart[] {
  const link = { label: 'Traces →', href: scoped('traces') };
  const source = `vala.traces.spans · service.name=${name} · service.version=${version} · ${step}s buckets`;
  const endsAt = new Date(end * 1000).toISOString();
  const base = (title: string, measure: string, unit: string) => ({ title, measure, unit, source, link, endsAt, series: [] });
  const specs = [
    base('REQUESTS', 'root spans', 'spans / bucket'),
    base('ERRORS', 'error root spans', 'spans / bucket'),
    base('LATENCY', 'mean root-span duration', 'ms')
  ];
  if (!traces.ok)
    return specs.map((c) => ({
      ...c,
      state: 'failed' as const,
      code: traces.code,
      note: traces.denied ? 'not authorized to read traces — not zero' : 'trace read failed — not zero'
    }));
  if (!traces.rows.length)
    return specs.map((c) => ({
      ...c,
      state: 'nodata' as const,
      note: `no root spans for ${name} ${version} in ${range}`
    }));
  const at = new Map(traces.rows.map((r) => [r.bucket, r]));
  const filled = Array.from({ length: BUCKETS }, (_, i) => at.get(i));
  const spans = filled.map((r) => r?.spans ?? 0);
  const errors = filled.map((r) => r?.errors ?? 0);
  const latency = filled.flatMap((r) => (r ? [Math.round(r.meanMs * 10) / 10] : []));
  const state = traces.degraded ? ('gap' as const) : ('ok' as const);
  const note = traces.degraded ? 'degraded read — some sources incomplete' : undefined;
  return [
    { ...specs[0], series: [{ label: 'spans', points: spans }], latest: `${spans.at(-1)} last bucket`, state, note },
    { ...specs[1], series: [{ label: 'errors', points: errors }], latest: `${errors.at(-1)} last bucket`, state, note },
    {
      ...specs[2],
      series: [{ label: 'mean ms', points: latency }],
      latest: `${latency.at(-1)} ms`,
      state,
      note: note ?? `${latency.length}/${BUCKETS} buckets observed · empty buckets omitted`
    }
  ];
}

/** Observe continuation for a Verifier by its implementation family. */
function observeLink(implementation: string, verifier: CardRef, scoped: (s: string, extra?: string) => string) {
  return implementation === 'drift'
    ? scoped('drift', `&driftCard=${encodeURIComponent(verifier.uid ?? '')}`)
    : scoped('evaluations', `&evalCard=${encodeURIComponent(verifier.uid ?? '')}`);
}

type Bound = { declared: DeclaredBinding; status: BindingStatus | null; rows: ResultRow[]; latest?: ResultRow };

/** One binding's published-intelligence panel from its own results. */
function signal(b: Bound, results: Read<ResultRow>, scoped: (s: string, extra?: string) => string): ServiceSignal {
  const { declared, status, rows, latest } = b;
  const title = `${declared.verifier.name.toUpperCase()} ${declared.verifier.version} → ${declared.alias}`;
  const subject = `subject ${declared.alias} (${label(declared.subject)}) · ${status?.readiness ?? 'status unavailable'}`;
  if (!results.ok && results.denied)
    return { title, subject, state: 'unauthorized', body: ['Not authorized to read verification results.', subject] };
  if (!results.ok)
    return {
      title,
      subject,
      verdict: { label: 'RESULTS UNAVAILABLE', tone: 'warn' },
      context: `results read failed · ${results.code}`
    };
  if (!latest)
    return { title, subject, state: 'nodata', body: [`No result for this binding in 30 days · ${subject}`] };
  const counts = Object.entries(
    rows.reduce<Record<string, number>>((m, r) => ({ ...m, [r.verdict]: (m[r.verdict] ?? 0) + r.results }), {})
  )
    .map(([v, n]) => `${n} ${v}`)
    .join(' · ');
  return {
    title,
    stamp: `latest ${clock(latest.lastEnded)}`,
    verdict: { label: latest.verdict.toUpperCase(), tone: verdictTone[latest.verdict] ?? 'warn' },
    subject,
    context: `${latest.implementation} · 30 days: ${counts}`,
    link: {
      label: latest.implementation === 'drift' ? 'Drift analysis →' : 'Evaluation results →',
      href: observeLink(latest.implementation, declared.verifier, scoped)
    }
  };
}

/** Declared lanes: components, their Verifiers, and on-failure Operators. Declaration only. */
function composition(
  card: Schemas['Card'],
  bindings: Bound[],
  components: { alias: string; ref: CardRef }[]
): ServicePresentation['composition'] {
  const unique = (refs: { ref: CardRef; note: string; status: string }[]): ServiceNode[] => {
    const nodes = new Map<string, ServiceNode>();
    for (const { ref, note, status } of refs) {
      const uid = ref.uid ?? label(ref);
      const node = nodes.get(uid);
      if (node) node.note = `${node.note}, ${note.split(' ').at(-1)}`;
      else nodes.set(uid, { uid, kind: ref.kind as CardKind, name: ref.name, version: ref.version, status, note });
    }
    return [...nodes.values()];
  };
  const self = card.metadata.uid ?? '';
  const runtime = unique(components.map(({ alias, ref }) => ({ ref, note: `alias ${alias}`, status: 'declared' })));
  const verifiers = unique(
    bindings.map(({ declared, status }) => ({
      ref: declared.verifier,
      note: `verifies ${declared.alias}`,
      status: status ? (status.readiness === 'ready' ? '✓ ready' : `✕ ${status.readiness}`) : 'status unavailable'
    }))
  );
  const operators = unique(
    bindings.flatMap(({ declared }) =>
      declared.onFailure.map((ref) => ({ ref, note: `on failure of ${declared.verifier.name}`, status: 'declared' }))
    )
  );
  const uidOf = (ref: CardRef) => ref.uid ?? label(ref);
  const edges = bindings.flatMap(({ declared }) => [
    {
      from: declared.subject.uid === self ? '__service' : uidOf(declared.subject),
      to: uidOf(declared.verifier),
      label: 'verified by'
    },
    ...declared.onFailure.map((op) => ({ from: uidOf(declared.verifier), to: uidOf(op), label: 'on failure' }))
  ]);
  return {
    intro: [
      `The declared composition of ${card.metadata.name} ${card.metadata.version}. Edges are declarations, not observed calls.`
    ],
    lanes: [
      { title: 'RUNTIME COMPOSITION', nodes: runtime, note: runtime.length ? undefined : 'No components declared.' },
      { title: 'MEASUREMENT', nodes: verifiers, note: verifiers.length ? undefined : 'No Verifier bindings declared.' },
      {
        title: 'REACTION',
        nodes: operators,
        note: operators.length ? undefined : 'No on-failure Operators declared — a failed result stays durable with no reaction.'
      }
    ],
    edges: [...new Map(edges.map((e) => [`${e.from}>${e.to}`, e])).values()],
    aliasNote: 'aliases are this Service’s local handles; one Card may serve several aliases',
    foot: 'read from this version’s declaration and its binding status'
  };
}

/** The declaration-first Definition straight from the registered spec. */
function definition(
  card: Schemas['Card'],
  components: { alias: string; ref: CardRef }[],
  bindings: DeclaredBinding[]
): ServicePresentation['definition'] {
  const spec = card.spec as ServiceSpec;
  const self = card.metadata.uid;
  const serviceLevel = bindings.filter((b) => b.subject.uid === self);
  const componentLevel = bindings.filter((b) => b.subject.uid !== self);
  return {
    declaration: {
      note: 'from the registered spec',
      summary: spec.description ?? `${card.metadata.name} declares ${components.length} components.`,
      entries: [
        { k: 'entry point', v: spec.entry_point ?? '—' },
        { k: 'service type', v: spec.service_type ?? '—' },
        { k: 'components', v: String(components.length) },
        { k: 'bindings', v: String(bindings.length) },
        { k: 'tables', v: (spec.tables ?? []).map((t) => t.name).join(', ') || 'none' }
      ]
    },
    components: {
      note: `${components.length} declared`,
      rows: components.map(({ alias, ref }) => {
        const verifiers = componentLevel.filter((b) => b.alias === alias).map((b) => b.verifier);
        return {
          alias,
          ref: `${ref.name} · ${ref.version}`,
          href: cardHref(ref),
          kind: ref.kind,
          publishes: verifiers.map((v) => `${v.name}@${v.version}`).join(', ') || '—',
          publishesHref: verifiers[0] ? cardHref(verifiers[0]) || undefined : undefined
        };
      }),
      aliasNote: 'each alias resolves to one exact Card version'
    },
    publications: {
      note: 'component bindings verify one component; Service bindings verify the whole Service',
      component: {
        label: 'COMPONENT BINDINGS',
        flows: componentLevel.map((b) => ({
          from: { label: b.alias, href: cardHref(b.subject) },
          to: { label: `${b.verifier.name}@${b.verifier.version}`, href: cardHref(b.verifier) }
        })),
        absent: 'No component-level bindings declared.'
      },
      service: {
        label: 'SERVICE BINDINGS',
        value: serviceLevel.map((b) => `${b.verifier.name}@${b.verifier.version}`).join(', ') || 'NONE DECLARED',
        note: serviceLevel.length ? 'verifies this Service as a whole' : 'This version declares no Service-level binding.'
      }
    },
    rawSpec: {
      summary: 'Show raw spec (JSON)',
      meta: card.metadata.spec_hash ?? '',
      yaml: JSON.stringify(spec, null, 2),
      note: 'credential references are names only; no secret value is stored in a spec'
    },
    principal: { note: 'server-managed', entries: [{ k: 'phase', v: card.status?.phase ?? 'unknown' }] },
    secrets: 'Secrets never render here: the Service declares credential references by name only.'
  };
}
