<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import Line from '$lib/components/charts/Line.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import ScopeChips from '$lib/features/observe/ScopeChips.svelte';
  import Select from '$lib/components/Select.svelte';
  import { RANGES, filterChips, scopeQuery } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/drift$/, ''));
  const chips = $derived(
    filterChips(page.url.pathname, {
      driftCard: data.filters.driftCard,
      service: data.filters.service,
      feature: data.filters.feature
    })
  );
  const rangeOptions = RANGES.map((r) => ({ value: r, label: r }));
  function featureHref(name: string): string {
    const query = new URLSearchParams(page.url.search);
    query.set('feature', name);
    return `?${query}`;
  }
</script>

<svelte:head><title>Drift · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Drift</h1>
  <p class="muted">Calculated drift reports against a declared Drift Card.</p>
  <ObserveNav base={`${base}/observe`} current="Drift" scope={data.scope} />
  <div class="stack">
    <form class="filters row" method="GET">
      <input type="hidden" name="driftCard" value={data.filters.driftCard} />
      <input type="hidden" name="service" value={data.filters.service} />
      <input type="hidden" name="feature" value={data.filters.feature} />
      <Select label="range" name="range" options={rangeOptions} value={data.filters.range} />
      <ScopeChips {chips} clearHref={page.url.pathname} />
    </form>
    {#if data.problem}
      <StateBlock
        state={data.problem.status === 403 ? 'unauthorized' : 'error'}
        title={data.problem.title}
        code={data.problem.code}
        actionLabel="Retry"
        actionHref={page.url.pathname + page.url.search}
      />
    {:else if data.view}
      <div class="columns">
        <div class="stack">
          <Panel title="Definition">
            {#snippet head()}<span class="mono muted">{data.view.card.name} ({data.view.card.id} {data.view.card.version}) · read-only reference</span>{/snippet}
            <div class="kv def-facts">
              <span class="k">method</span><span class="mono"><strong>{data.view.definition.method}</strong></span>
              <span class="k">baseline</span><span class="mono"><strong>{data.view.definition.baseline}</strong></span>
              <span class="k">window</span><span class="mono"><strong>{data.view.definition.window}</strong></span>
            </div>
            <p class="muted">condition: {data.view.definition.condition}</p>
          </Panel>
          {#if !data.view.report}
            <StateBlock
              state="absent"
              title="No report for this range"
              detail={`No calculated report exists inside ${data.filters.range}. Reports are calculated on the Card's own window — widen the range to 30d. Absence of a report is not a passing verdict.`}
              actionLabel="View the 30d window"
              actionHref={`?${new URLSearchParams({ ...Object.fromEntries(new URLSearchParams(page.url.search)), range: '30d' })}`}
            />
          {:else}
            {@const report = data.view.report}
            <Panel variant="accent" title={report.verdict.label}>
              {#snippet head()}<Badge tone={report.verdict.tone}>report</Badge>{/snippet}
              <p>{report.summary}</p>
              <p class="mono muted">
                report {report.id} · calculated {report.calculated} — a
                calculated report, not a raw observation
              </p>
            </Panel>
            <Panel title={`Score history — ${report.selectedFeature.name}`}>
              {#snippet head()}<span class="mono muted">{report.selectedFeature.method} · {data.filters.range}</span>{/snippet}
              <Line
                series={[{ label: report.selectedFeature.method, points: report.history.points }]}
                labels={report.history.labels}
                threshold={report.history.threshold}
              />
              <p class="muted">
                threshold has a positional tick · a day with no report is a labeled gap, not a zero
              </p>
            </Panel>
            <Panel title="Features" variant="raised">
              {#snippet head()}<span class="mono muted">{report.features.length} · selected {report.selectedFeature.name}</span>{/snippet}
              <Table label="Per-feature drift results">
                <table>
                  <thead><tr><th>Feature</th><th>Method</th><th>Score</th><th>Threshold</th><th>Verdict</th></tr></thead>
                  <tbody>
                    {#each report.features as feature (feature.name)}
                      <tr class={report.selectedFeature.name === feature.name ? 'sel' : ''}>
                        <td class="mono"><a href={featureHref(feature.name)}><strong>{feature.name}</strong></a></td>
                        <td class="mono">{feature.method}</td>
                        <td class={`mono ${feature.verdict.tone === 'danger' ? 'error' : ''}`}><strong>{feature.score.toFixed(2)}</strong></td>
                        <td class="mono">{feature.threshold.toFixed(2)}</td>
                        <td><Badge tone={feature.verdict.tone}>{feature.verdict.label}</Badge></td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </Table>
            </Panel>
          {/if}
        </div>
        <div class="stack">
          {#if data.view.report?.alert}
            <Panel title="Alert" variant="raised">
              {#snippet head()}<span class="mono muted">threshold breach</span>{/snippet}
              <p><Badge tone="danger">{data.view.report.alert.finding}</Badge></p>
              <div class="kv">
                <span class="k">breached</span><span class="mono">{data.view.report.alert.breached}</span>
                <span class="k">reaction</span>
                <a class="mono" href={`${base}/cards/${data.view.report.alert.operatorHref}`}>{data.view.report.alert.reaction}</a>
              </div>
              <p class="muted">The alert links to the Operator that owns the reaction — never to an alert editor.</p>
            </Panel>
          {/if}
          <div class="notice">
            <p class="mono muted">RAW OBSERVATIONS</p>
            <p class="muted">
              Raw drift input observations are distinct from these calculated reports; they never
              substitute for one another.
            </p>
          </div>
          <Panel title="Correlate">
            {#snippet head()}<span class="mono muted">same service</span>{/snippet}
            <div class="stack correlate">
              <a class="mono" href={`${base}/observe/logs${scopeQuery({ service: data.filters.service, range: '1h' })}`}>Logs · {data.filters.service || 'all services'}</a>
              <a class="mono" href={`${base}/observe/metrics${scopeQuery({ service: data.filters.service, range: '6h' })}`}>Metrics · {data.filters.service || 'all services'}</a>
              <a class="mono" href={`${base}/observe/traces${scopeQuery({ service: data.filters.service, range: '1h' })}`}>Traces · {data.filters.service || 'all services'}</a>
              <a class="mono" href={`${base}/cards/${data.view.card.href}`}>{data.view.card.name} Card</a>
            </div>
          </Panel>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .def-facts {
    grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr) auto minmax(0, 1fr);
  }
  .correlate {
    gap: 6px;
  }
  @media (max-width: 700px) {
    .def-facts {
      grid-template-columns: auto minmax(0, 1fr);
    }
  }
</style>
