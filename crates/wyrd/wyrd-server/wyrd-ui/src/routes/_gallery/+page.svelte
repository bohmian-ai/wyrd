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
    Sparkline,
    StatusPill,
    VerdictPill,
    type ActionIntent,
    type ActionState
  } from 'wyrd-ai-ui';
  import { WyrdView } from 'wyrd-ai-ui/view';
  import { card, day, denied, fixtureExecutor, registered, series, summary, terminal, timeseries } from '$lib/ui-fixtures';

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

  const table = { ...series, terminal: terminal(4, 'degraded') };
  const versions = { state: 'populated', data: { items: [summary('1.4.2'), summary('1.4.1', 'deprecated')], truncated: true } } as const;
  const doc = {
    title: 'Ranker overview',
    range: { last: '6h' },
    panels: [
      { id: 'header', component: 'CardHeader', input: { cardRef: card } },
      { id: 'latency', component: 'Chart', input: timeseries },
      { id: 'broken', component: 'Chart', input: { type: 'stat' } }
    ]
  };
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
        <Chart input={{ type: 'stat', title: 'Runs today', sql: 'SELECT count(*) FROM t', unit: 'runs' }} result={{ state: 'populated', data: { columns: [series.columns[2]], rows: [[12400]], terminal: terminal(1) } }} />
        <Chart input={{ type: 'table', title: 'Partial table', sql: 'SELECT * FROM t' }} result={{ state: 'populated', data: table }} {actions} {onaction} />
        <Chart input={timeseries} result={{ state: 'empty', data: { ...series, rows: [] }, reason: 'The SQL returned no rows for p95 in this range.' }} {actions} {onaction} />
        <Chart input={timeseries} result={{ state: 'loading' }} {actions} {onaction} />
        <Chart input={timeseries} result={{ state: 'denied', problem: denied }} {actions} {onaction} />
        <Chart input={timeseries} result={{ state: 'error', problem: { code: 'WYRD_VALA_413_QUERY_RESULT_TOO_LARGE', title: 'Query result too large' } }} {actions} {onaction} />
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
      <WyrdView {doc} executor={fixtureExecutor} {onaction} />
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
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
</style>
