<!--
  Development-only gallery: every wyrd-ai-ui component in every state, in
  light and dark, from the shared fixtures. Actions resolve locally so
  pending, success and refusal can be inspected.
-->
<script lang="ts">
  import {
    CardHeader,
    CardRefChip,
    Chart,
    Drawer,
    DurationBar,
    EmptyState,
    FacetRail,
    RecordDetail,
    RecordTable,
    Sparkline,
    StatStrip,
    TraceWaterfall,
    StatusPill,
    VerdictPill,
    type ActionIntent,
    type ActionState
  } from 'wyrd-ai-ui';
  import { WyrdView } from 'wyrd-ai-ui/view';
  import {
    buckets,
    card,
    costs,
    day,
    denied,
    facet,
    fixtureExecutor,
    LOG,
    logRow,
    registered,
    series,
    spanRow,
    summary,
    terminal,
    timeseries,
    total,
    trace,
    TRACE,
    traceCounts,
    traces
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

  const SPAN = 'b9c7c989f97918e1';
  const stats = [
    { stat: 'traces', total: total(128), series: buckets },
    { stat: 'errors', total: total(3), series: buckets },
    { stat: 'duration_p95', total: total(2_840_000_000), series: buckets },
    { stat: 'tokens', total: total(null), series: { ...buckets, rows: [] } },
    { stat: 'cost', total: costs, series: buckets }
  ] as const;
  const facets = [
    { column: 'service_name', values: facet },
    { column: 'status_code', values: { ...facet, rows: [[0, 120], [2, 8]] } },
    { column: 'gen_ai_request_model', values: { ...facet, rows: [] } }
  ];
  const where = [{ column: 'service_name', value: 'agent-api' }];
  /** Holds the live list, as an open trace or a scrolled reader would. */
  let held = $state(false);
  let newest = $state(false);
  const latest = $derived({
    ...traces,
    rows: newest ? [['2026-10-20T11:59:30.000000000+00:00', 'invoke_agent billing', 'agent-api', 1_310_000_000, 1, 'ffffffffffffffffffffffffffffffff', '1111111111111111'], ...traces.rows] : traces.rows
  });
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

      <h2>Observe</h2>
      <div class="observe">
        <FacetRail input={{ signal: 'traces', facets: facets.map((each) => each.column), where }} result={{ state: 'populated', data: { facets } }} {actions} {onaction} />
        <div class="stack">
          <StatStrip input={{ stats: stats.map((each) => each.stat) }} result={{ state: 'populated', data: { stats } }} />
          <div class="row">
            <label><input type="checkbox" bind:checked={held} /> Hold (trace open)</label>
            <button type="button" class="open" onclick={() => (newest = !newest)}>{newest ? 'Drop' : 'Deliver'} a new trace</button>
          </div>
          <RecordTable
            input={{ signal: 'traces', live: true }}
            result={{ state: 'populated', data: { rows: latest, counts: { state: 'populated', data: traceCounts } } }}
            paused={held}
            {actions}
            {onaction}
          />
          <RecordTable input={{ signal: 'spans', groupBy: 'service_name', columns: ['start_time_unix_nano', 'name', 'service_name', 'duration_nano', 'status_code'] }} result={{ state: 'populated', data: { rows: traces } }} {actions} {onaction} />
          <RecordTable input={{ signal: 'traces' }} result={{ state: 'populated', data: { rows: traces, counts: { state: 'denied', problem: denied } } }} />
        </div>
      </div>
      <div class="grid">
        <TraceWaterfall input={{ traceId: TRACE, spanId: SPAN }} result={{ state: 'populated', data: trace }} {actions} {onaction} />
        <RecordDetail input={{ traceId: TRACE, spanId: SPAN }} result={{ state: 'populated', data: spanRow }} {actions} {onaction} />
        <RecordDetail input={{ log: LOG }} result={{ state: 'populated', data: logRow }} {actions} {onaction} />
        <RecordDetail input={{ traceId: TRACE, spanId: SPAN }} result={{ state: 'populated', data: { ...spanRow, rows: [[...spanRow.rows[0].slice(0, 4), {}, [], null, null, null]] } }} />
      </div>
      <div class="grid">
        <StatStrip input={{ stats: ['spans'] }} result={{ state: 'empty', data: { stats: [{ stat: 'spans', total: total(0), series: { ...buckets, rows: [] } }] }, reason: 'Nothing recorded in this range.' }} />
        <FacetRail input={{ signal: 'logs', facets: ['severity_text'] }} result={{ state: 'loading' }} />
        <RecordTable input={{ signal: 'logs' }} result={{ state: 'empty', data: { rows: { ...traces, rows: [] } }, reason: 'No logs match this WHERE in this range.' }} />
        <TraceWaterfall input={{ traceId: TRACE }} result={{ state: 'empty', data: { ...trace, rows: [] }, reason: `No retained span you may read has trace_id ${TRACE}.` }} />
        <RecordDetail input={{ traceId: TRACE, spanId: SPAN }} result={{ state: 'denied', problem: denied }} />
        <TraceWaterfall input={{ traceId: TRACE }} result={{ state: 'error', problem: { code: 'WYRD_VALA_413_QUERY_RESULT_TOO_LARGE', title: 'Query result too large' } }} />
      </div>

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
  .observe {
    display: grid;
    grid-template-columns: minmax(180px, 240px) minmax(0, 1fr);
    gap: 16px;
  }
  .stack {
    display: grid;
    gap: 12px;
    min-width: 0;
  }
  @media (max-width: 720px) {
    .observe {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
</style>
