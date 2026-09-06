<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Panel from '$lib/components/Panel.svelte';
  import Select from '$lib/components/Select.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import Bars from '$lib/components/charts/Bars.svelte';
  import Line from '$lib/components/charts/Line.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import { RANGES, readScope } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/dashboards\/[^/]+$/, ''));
  const scope = $derived(readScope(page.url.searchParams, '24h'));
</script>

<svelte:head><title>{data.view?.name ?? 'Dashboard'} · Observe · Wyrd</title></svelte:head>
<div class="observe">
  {#if data.problem}
    <h1>Dashboard</h1>
    <ObserveNav base={`${base}/observe`} current="Dashboards" {scope} />
    <StateBlock
      state={data.problem.status === 403 ? 'unauthorized' : data.problem.status === 404 ? 'absent' : 'error'}
      title={data.problem.title}
      code={data.problem.code}
      actionLabel="Back to dashboards"
      actionHref={`${base}/observe/dashboards`}
    />
  {:else if data.view}
    <h1>{data.view.name}</h1>
    <p class="muted">A read-only panel grid with a global range and variables.</p>
    <ObserveNav base={`${base}/observe`} current="Dashboards" {scope} />
    <div class="stack">
      <a class="mono" href={`${base}/observe/dashboards`}>← Back to dashboards</a>
      <p class="mono muted">
        owner {data.view.owner} · updated {data.view.updated} · read-only — no editor, no save, no
        panel drag · variables apply to every panel
      </p>
      <form class="row" method="GET">
        {#each data.view.variables as variable (variable.name)}
          <Select
            label={variable.name}
            name={variable.name}
            value={variable.value}
            options={variable.options.map((option) => ({ value: option, label: option }))}
          />
        {/each}
        <Select
          label="range"
          name="range"
          value={scope.range}
          options={RANGES.map((range) => ({ value: range, label: range }))}
        />
      </form>
      <div class="panel-grid">
        {#each data.view.panels as panel (panel.title)}
          <Panel title={panel.title}>
            {#snippet head()}<span class="mono muted">{panel.meta}</span>{/snippet}
            {#if panel.state === 'empty'}
              <StateBlock state="empty" title="No data for this range" detail={panel.note} />
            {:else if panel.state === 'error'}
              <StateBlock state="error" title="Panel query failed" detail={panel.note} code={panel.code} />
            {:else if panel.kind === 'line' && panel.series}
              <Line series={panel.series} labels={panel.labels ?? []} threshold={panel.threshold} />
            {:else if panel.kind === 'bars' && panel.bars}
              <Bars data={panel.bars} />
            {:else if panel.kind === 'table' && panel.table}
              <Table label={panel.title}>
                <table>
                  <thead><tr>{#each panel.table.columns as column (column)}<th>{column}</th>{/each}</tr></thead>
                  <tbody>
                    {#each panel.table.rows as row, i (i)}
                      <tr>{#each row as cell, j (j)}<td class="mono">{cell}</td>{/each}</tr>
                    {/each}
                  </tbody>
                </table>
              </Table>
            {/if}
          </Panel>
        {/each}
      </div>
      <p class="muted">
        A per-panel state is a state of the live grid — no-data and safe-error render in place, at
        panel size.
      </p>
    </div>
  {/if}
</div>
