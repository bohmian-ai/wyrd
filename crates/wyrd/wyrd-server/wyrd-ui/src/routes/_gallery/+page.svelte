<!--
  Development-only gallery: every wyrd-ai-ui component in every state, in
  light and dark, from the shared fixtures. Actions resolve locally so
  pending, success and refusal can be inspected. Each Observe component is one
  checkpoint naming the canvas boards and regions it must match; every state
  renders through WyrdView from a fixture view document.
-->
<script lang="ts">
  import {
    CardHeader,
    CardRefChip,
    Chart,
    Drawer,
    DurationBar,
    EmptyState,
    Sparkline,
    StatusPill,
    VerdictPill,
    type ActionIntent,
    type ActionState
  } from 'wyrd-ai-ui';
  import { WyrdView } from 'wyrd-ai-ui/view';
  import type { Executor } from 'wyrd-ai-ui';
  import type { ComponentName } from 'wyrd-ai-ui/loaders';
  import {
    AUTHORED_SQL,
    bareSpan,
    card,
    day,
    denied,
    DRIFT_VERIFIER,
    fixtureExecutor,
    liveExecutor,
    LOG,
    NATIVE,
    PROMPT,
    RANGE,
    registered,
    series,
    SESSION_VERIFIER,
    stateExecutors,
    summary,
    terminal,
    timeseries,
    TRACE,
    VERIFIER
  } from '$lib/ui-fixtures';

  let drawer = $state(false);
  let actions = $state<Record<string, ActionState>>({});

  /** Hold each intent pending briefly, then answer success or, for Observe, a refusal. */
  function onaction(intent: ActionIntent) {
    actions[intent.name] = { state: 'pending' };
    setTimeout(() => {
      actions[intent.name] =
        intent.name === 'card.open_in_observe' ? { state: 'denied', problem: denied } : { state: 'success', message: `${intent.name} sent` };
    }, 800);
  }

  /** A host action answered after a pause, so pending and success can be seen. */
  const settle = (intent: ActionIntent) =>
    new Promise<ActionState>((resolve) => setTimeout(() => resolve({ state: 'success', message: `${intent.name} sent` }), 600));

  type State = keyof typeof stateExecutors;
  /** One rendered case of a checkpoint: a component input under one executor. */
  interface Case {
    readonly label: string;
    readonly input: unknown;
    readonly state?: State;
    readonly executor?: Executor;
    readonly narrow?: true;
  }
  /** One component's checkpoint: the board regions it must match and its cases. */
  interface Checkpoint {
    readonly component: ComponentName;
    readonly regions: readonly (readonly [board: string, region: string])[];
    readonly note?: string;
    readonly cases: readonly Case[];
  }
  /** The non-data states every data component shows. */
  const STATES: readonly State[] = ['empty', 'loading', 'denied', 'error'];
  const states = (input: unknown): Case[] => STATES.map((state) => ({ label: state, input, state }));
  const SPAN = 'b9c7c989f97918e1';
  const bare: Executor = { run: (requests) => Promise.resolve(requests.map(() => ({ ok: true, value: bareSpan }))) as never };

  const checkpoints = (live: Executor): Checkpoint[] => [
    {
      component: 'CardInventory',
      regions: [['Cards', 'filter bar, kind nav, version table and footer']],
      cases: [
        { label: 'all kinds · first page, more to load', input: {} },
        { label: 'two pages kept after Load more', input: { pages: 2 } },
        { label: 'filtered to Prompts with prereleases', input: { kind: 'Prompt', include_prerelease: true, query: 'support' } },
        { label: 'filtered · no match', input: { query: 'nothing' }, state: 'empty' },
        { label: 'empty tenant', input: {}, state: 'empty' },
        ...STATES.filter((state) => state !== 'empty').map((state) => ({ label: state, input: {}, state })),
        { label: 'narrow', input: {}, narrow: true }
      ]
    },
    {
      component: 'CardFacts',
      regions: [['Prompt', 'Metadata rail']],
      cases: [
        { label: 'metadata', input: { cardRef: PROMPT } },
        { label: 'YAML', input: { cardRef: PROMPT, yaml: true } },
        ...STATES.filter((state) => state !== 'empty').map((state) => ({ label: state, input: { cardRef: PROMPT }, state }))
      ]
    },
    {
      component: 'RelationshipList',
      regions: [['Prompt', 'Relationships rail']],
      cases: [{ label: 'outbound and inbound', input: { cardRef: { kind: 'Agent', name: 'support', version: '1.0.0' } } }, ...states({ cardRef: PROMPT })]
    },
    {
      component: 'ArtifactTable',
      regions: [['Cards', 'Files tab']],
      cases: [{ label: 'registered files', input: { cardRef: card } }, ...states({ cardRef: card })]
    },
    {
      component: 'PromptMessages',
      regions: [['Prompt', 'Prompt panel']],
      cases: [
        { label: 'portable', input: { cardRef: PROMPT } },
        { label: 'native body with an extension field', input: { cardRef: NATIVE } },
        ...STATES.filter((state) => state !== 'empty').map((state) => ({ label: state, input: { cardRef: PROMPT }, state }))
      ]
    },
    {
      component: 'PromptVariables',
      regions: [['Prompt', 'Variables table']],
      cases: [{ label: 'derived variables', input: { cardRef: PROMPT } }, ...states({ cardRef: PROMPT })]
    },
    {
      component: 'UsedBy',
      regions: [['Prompt', 'Used by table']],
      cases: [{ label: 'one Agent behind a Service', input: { cardRef: PROMPT } }, ...states({ cardRef: PROMPT })]
    },
    {
      component: 'VersionDiff',
      regions: [['Prompt', 'Changes from the previous version']],
      cases: [{ label: '1.0.0 → 1.1.0', input: { cardRef: PROMPT } }, { label: 'only one version', input: { cardRef: PROMPT }, state: 'empty' }, ...STATES.filter((state) => state !== 'empty').map((state) => ({ label: state, input: { cardRef: PROMPT }, state }))]
    },
    {
      component: 'VerifierDefinition',
      regions: [['Verification mock · Verifier page', 'description, Graph panel and declaration inspector']],
      note: 'Definition only: no execution colour. Select a node, use the arrow keys, zoom and Fit.',
      cases: [
        { label: 'parallel steps, fan-in, gate, retries and both requirement positions', input: { cardRef: VERIFIER } },
        { label: 'requirements only: a compact panel', input: { cardRef: { ...VERIFIER, version: '1.0.0' } } },
        { label: 'session judgment over group sources', input: { cardRef: SESSION_VERIFIER } },
        { label: 'drift check over samples', input: { cardRef: DRIFT_VERIFIER } },
        ...STATES.filter((state) => state !== 'empty').map((state) => ({ label: state, input: { cardRef: VERIFIER }, state })),
        { label: 'narrow: the graph pans', input: { cardRef: VERIFIER }, narrow: true }
      ]
    },
    {
      component: 'RunsOn',
      regions: [['Verification mock · Verifier page', 'Runs on rail']],
      cases: [{ label: 'a Service binding itself and one component', input: { cardRef: VERIFIER } }, ...states({ cardRef: VERIFIER })]
    },
    {
      component: 'FacetRail',
      regions: [['Main', 'left facet column'], ['Spans', 'left facet column'], ['Logs', 'left facet column']],
      cases: [
        { label: 'traces · a value in WHERE', input: { signal: 'traces', facets: ['workload_card_uid', 'subject_card_uid', 'service_name', 'status_code'], where: [{ column: 'service_name', value: 'agent-api' }] } },
        { label: 'spans · more values than shown', input: { signal: 'spans', facets: ['name', 'service_name', 'status_code'] } },
        { label: 'logs', input: { signal: 'logs', facets: ['severity_text', 'workload_card_uid', 'service_name'] } },
        ...states({ signal: 'traces', facets: ['service_name'] }),
        { label: 'narrow', input: { signal: 'spans', facets: ['name', 'status_code'] }, narrow: true }
      ]
    },
    {
      component: 'RecordTable',
      regions: [['Main', 'trace list'], ['Spans', 'flat span list'], ['Logs', 'records list']],
      cases: [
        { label: 'traces · live: hover or scroll holds, a new trace arrives every 5s', input: { signal: 'traces', live: true }, executor: live },
        { label: 'spans · HTTP and misleading names, unset service', input: { signal: 'spans' } },
        { label: 'spans · grouped by service: unset and the text "∅" stay apart', input: { signal: 'spans', groupBy: 'service_name' } },
        { label: 'spans · grouped by duration: two values that read "1ms" stay apart', input: { signal: 'spans', groupBy: 'duration_nano' } },
        { label: 'logs · 120 read, paged, wrap and columns', input: { signal: 'logs', limit: 120 } },
        { label: 'metrics · null is not zero', input: { signal: 'metrics' } },
        { label: 'authored SQL · aliases keep their returned meaning', input: { sql: AUTHORED_SQL } },
        ...states({ signal: 'logs' }),
        { label: 'narrow', input: { signal: 'spans' }, narrow: true }
      ]
    },
    {
      component: 'Chart',
      regions: [['Metrics', 'series summary and chart-line visibility']],
      note: 'Reused Chart: Latest, Max and Mean of plotted values; unticking a series hides its line.',
      cases: [{ label: 'two series', input: timeseries }, { label: 'narrow', input: timeseries, narrow: true }]
    },
    {
      component: 'TraceWaterfall',
      regions: [['TraceDrawer', 'upper span tree and timeline']],
      cases: [
        { label: 'selected span', input: { traceId: TRACE, spanId: SPAN } },
        { label: 'deep selection opens its path', input: { traceId: TRACE, spanId: 'c1d2e3f405060708' } },
        ...states({ traceId: TRACE }),
        { label: 'narrow', input: { traceId: TRACE }, narrow: true }
      ]
    },
    {
      component: 'RecordDetail',
      regions: [['TraceDrawer', 'lower selected-span detail'], ['Logs', 'opened-record body']],
      cases: [
        { label: 'span · messages, attributes, exception', input: { traceId: TRACE, spanId: SPAN } },
        { label: 'span · nothing recorded', input: { traceId: TRACE, spanId: SPAN }, executor: bare },
        { label: 'log record', input: { log: LOG } },
        ...states({ traceId: TRACE, spanId: SPAN }),
        { label: 'narrow', input: { traceId: TRACE, spanId: SPAN }, narrow: true }
      ]
    },
    {
      component: 'StatStrip',
      regions: [['Agent', 'Invocations and Model usage'], ['Service', 'Signals'], ['Home', 'Observe summary']],
      note: 'Unresolved: the inventory says numbers with sparklines; these boards show different presentations. Not accepted until resolved (task audit, StatStrip).',
      cases: [
        { label: 'span stats', input: { stats: ['traces', 'errors', 'duration_p95'] } },
        { label: 'gateway stats scoped to a workload · nothing recorded reads —', input: { stats: ['invocations', 'tokens', 'cost'], scope: { column: 'workload_card_uid', uid: 'card_01HX' } } },
        ...states({ stats: ['traces'] }),
        { label: 'narrow', input: { stats: ['traces', 'errors', 'cost'] }, narrow: true }
      ]
    }
  ];
  /** One document per case, built once so a re-render never reloads it. */
  const views = Object.fromEntries(
    (['light', 'dark'] as const).map((mode) => [
      mode,
      checkpoints(liveExecutor()).map((checkpoint) => ({
        ...checkpoint,
        cases: checkpoint.cases.map((each) => ({
          ...each,
          executor: each.executor ?? stateExecutors[each.state ?? 'populated'],
          doc: { version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1', range: RANGE, panels: [{ id: checkpoint.component, component: checkpoint.component, input: each.input }] }
        }))
      }))
    ])
  );
  const table = { ...series, terminal: terminal(4, 'degraded') };
  const versions = { state: 'populated', data: { items: [summary('1.4.2'), summary('1.4.1', 'deprecated')], truncated: true } } as const;
  /** Placed panels; edits replace the document for this visit only. */
  let doc = $state.raw<unknown>({
    version: 'wyrd.view/v1',
    catalogId: 'wyrd.ui/v1',
    title: 'Ranker overview',
    range: { from: '2026-10-20T00:00:00Z', to: '2026-10-21T00:00:00Z' },
    panels: [
      { id: 'header', component: 'CardHeader', input: { cardRef: card }, layout: { x: 0, y: 0, w: 12, h: 4 } },
      { id: 'latency', component: 'Chart', input: timeseries, layout: { x: 0, y: 4, w: 8, h: 8 } },
      { id: 'broken', component: 'Chart', input: { type: 'stat' }, layout: { x: 8, y: 4, w: 4, h: 4 } },
      { id: 'unplaced', component: 'Chart', input: { ...timeseries, title: 'Unplaced: full width at content height' } }
    ]
  });
</script>

<svelte:head><title>Component gallery</title></svelte:head>

<main>
  {#each ['light', 'dark'] as mode (mode)}
    <section data-mode={mode} class="board">
      <h1>{mode}</h1>

      <h2>CardHeader</h2>
      <CardHeader
        input={{ cardRef: card, tabs: [{ id: 'overview', label: 'Overview' }, { id: 'runs', label: 'Runs' }] }}
        result={{ state: 'populated', data: { card: registered, versions } }}
        {actions}
        {onaction}
      />
      <CardHeader input={{ cardRef: card }} result={{ state: 'populated', data: { card: registered, versions: { state: 'denied', problem: denied } } }} />
      <CardHeader input={{ cardRef: card }} result={{ state: 'loading' }} />
      <CardHeader input={{ cardRef: card }} result={{ state: 'denied', problem: denied }} />

      <h2>Chart</h2>
      <div class="grid">
        <Chart input={timeseries} executed={{ sql: timeseries.sql, params: [] }} result={{ state: 'populated', data: day }} {actions} {onaction} />
        <Chart input={{ type: 'bar', title: 'Runs by model', sql: 'SELECT model, runs FROM t', x: 'model', y: ['p95'] }} result={{ state: 'populated', data: { ...series, columns: [series.columns[1], series.columns[2]], rows: [['a', 120], ['b', 80]] } }} {actions} {onaction} />
        <Chart input={{ type: 'bar', title: 'p95 by hour', sql: 'SELECT at, p95 FROM t', x: 'at', y: ['p95'] }} result={{ state: 'populated', data: { ...day, columns: [day.columns[0], day.columns[2]], rows: day.rows.filter((_, index) => index % 2 === 0).map((row) => [row[0], row[2]]) } }} {actions} {onaction} />
        <Chart input={{ type: 'stat', title: 'Runs today', sql: 'SELECT count(*) FROM t', unit: 'runs' }} result={{ state: 'populated', data: { columns: [series.columns[2]], rows: [[12400]], terminal: terminal(1) } }} />
        <Chart input={{ type: 'table', title: 'Partial table', sql: 'SELECT * FROM t' }} result={{ state: 'populated', data: table }} {actions} {onaction} />
        <Chart input={timeseries} result={{ state: 'empty', data: { ...series, rows: [] }, reason: 'The SQL returned no rows for p95 in this range.' }} {actions} {onaction} />
        <Chart input={timeseries} result={{ state: 'loading' }} {actions} {onaction} />
        <Chart input={timeseries} result={{ state: 'denied', problem: denied }} {actions} {onaction} />
        <Chart input={timeseries} result={{ state: 'error', problem: { code: 'WYRD_VALA_413_QUERY_RESULT_TOO_LARGE', title: 'Query result too large' } }} {actions} {onaction} />
      </div>

      <h2>Catalog components</h2>
      {#each views[mode] as checkpoint (checkpoint.component)}
        <section class="checkpoint" aria-label="{checkpoint.component} checkpoint">
          <h3>{checkpoint.component}</h3>
          <p class="regions">
            {#each checkpoint.regions as [board, region], index (index)}
              {#if index > 0}{' · '}{/if}<a href="/_gallery/board/{board}" target="_blank" rel="noreferrer">{board}</a> — {region}
            {/each}
          </p>
          {#if checkpoint.note}<p class="note">{checkpoint.note}</p>{/if}
          <div class="cases">
            {#each checkpoint.cases as each (each.label)}
              <figure class:narrow={each.narrow} class:wide={!each.narrow && checkpoint.component !== 'FacetRail'}>
                <figcaption>{each.label}</figcaption>
                <WyrdView doc={each.doc} executor={each.executor} onaction={settle} />
              </figure>
            {/each}
          </div>
        </section>
      {/each}

      <h2>Primitives</h2>
      <div class="row">
        <StatusPill label="neutral" /><StatusPill tone="ok" label="active" /><StatusPill tone="warn" label="deprecated" />
        <StatusPill tone="danger" label="failed" /><StatusPill tone="running" label="pending" />
        <VerdictPill verdict="passed" /><VerdictPill verdict="failed" /><VerdictPill verdict="inconclusive" />
        <CardRefChip cardRef={card} /><CardRefChip cardRef={card} href="#chip" />
        <Sparkline points={[3, 5, 4, 8, 6, 9]} label="Runs this week" />
      </div>
      <DurationBar ms={4210} max={8420} label="llm.call" />
      <DurationBar ms={843} max={8420} label="tool.search with a long span name that must ellipsize" />
      <div class="grid">
        <EmptyState state="loading" title="Loading" />
        <EmptyState state="empty" title="No runs" detail="Nothing ran in this range." />
        <EmptyState state="denied" problem={denied} />
        <EmptyState state="error" problem={{ code: 'WYRD_SPEC_500_INTERNAL', title: 'Internal error' }} />
      </div>
      <button type="button" class="open" onclick={() => (drawer = true)}>Open drawer</button>

      <h2>View document</h2>
      <WyrdView {doc} executor={fixtureExecutor} {onaction} onlayout={(next) => (doc = next)} />
    </section>
  {/each}
  <Drawer open={drawer} title="Span detail" onclose={() => (drawer = false)}><p>Drawer body.</p></Drawer>
</main>

<style>
  main {
    display: grid;
  }
  .board {
    display: grid;
    gap: 12px;
    align-content: start;
    min-width: 0;
    padding: 24px;
    background: var(--bg);
    color: var(--text);
  }
  h1,
  h2 {
    margin: 8px 0 0;
    font: var(--weight-strong) 14px var(--font-sans);
    text-transform: capitalize;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 420px), 1fr));
    gap: 12px;
  }
  .open {
    justify-self: start;
    height: 34px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 12px;
    background: var(--surface-2);
    color: var(--text);
    font: 13px var(--font-sans);
    cursor: pointer;
  }
  .open:hover {
    background: var(--surface-hover);
  }
  .checkpoint {
    display: grid;
    gap: 8px;
    border-top: 1px solid var(--border);
    padding-top: 12px;
  }
  h3 {
    margin: 0;
    font: 13px var(--font-mono);
  }
  .regions,
  .note {
    margin: 0;
    color: var(--muted);
    font: 12px var(--font-sans);
  }
  .note {
    color: var(--warn);
  }
  .cases {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 260px), 1fr));
    gap: 16px;
    align-items: start;
  }
  figure {
    display: grid;
    gap: 6px;
    min-width: 0;
    margin: 0;
  }
  .wide {
    grid-column: 1 / -1;
  }
  .narrow {
    max-width: 360px;
  }
  figcaption {
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
</style>
