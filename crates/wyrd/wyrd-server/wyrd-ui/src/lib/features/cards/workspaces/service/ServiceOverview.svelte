<script lang="ts">
  // The default operational view (C-09/S-01/S-04/S-05/S-06/S-07): one
  // compact server-projected assessment, four separated state channels, the
  // dominant shared-range chart grid, published Drift/Eval intelligence from
  // the declaration bindings, and one subordinate context band. Every state
  // is rendered from the projection — never computed here (REQ-124).
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import ChartPanel from '$lib/components/charts/ChartPanel.svelte';
  import Line from '$lib/components/charts/Line.svelte';
  import type { CardDetail, ServiceChart, ServicePresentation } from '../../core/types';
  import {
    chartWindow,
    observeHref,
    serviceHref,
    type ServiceScope,
    type WindowStamp
  } from './service-state';

  let {
    spec,
    detail,
    base,
    scope,
    path
  }: {
    spec: ServicePresentation;
    detail: CardDetail;
    base: string;
    scope: ServiceScope;
    path: string;
  } = $props();

  /** The server-projected state variant the URL selects; default otherwise. */
  const view = $derived(spec.overview.variants[scope.state] ?? spec.overview.default);

  /** ChartPanel state for each server-projected chart state. */
  const panelStates = {
    ok: 'ok',
    gap: 'partial',
    failed: 'error',
    nodata: 'empty'
  } as const;

  /** Axis stamps for one chart — a gap truthfully ends where data stops. */
  function windowFor(chart: ServiceChart): { from: WindowStamp; mid: WindowStamp; to: WindowStamp } {
    return chartWindow(scope.range, spec.now, chart.endsAt);
  }

  /** Per-point x labels for Line: first, midpoint and last tick only. */
  function pointLabels(chart: ServiceChart): WindowStamp[] {
    const points = chart.series[0]?.points.length ?? 0;
    const { from, mid, to } = windowFor(chart);
    return Array.from({ length: points }, (_, i) =>
      i === 0
        ? from
        : i === Math.floor((points - 1) / 2)
          ? mid
          : i === points - 1
            ? to
            : { label: '', at: new Date(Date.parse(from.at) + i).toISOString() }
    );
  }

  /** Map a context-band view token (`composition`/`definition`) to its href. */
  function contextHref(token: string): string {
    return serviceHref(path, detail.version, scope, {
      view: token === 'definition' ? 'definition' : 'composition'
    });
  }
</script>

<section class="assess" data-tone={view.assessment.tone} aria-label="Operational assessment">
  <div class="headline">
    <span class="title">{view.assessment.title}</span>
    <span>{view.assessment.sentence}</span>
    <span class="mono muted meta">{view.assessment.meta}</span>
    {#if view.assessment.retry}
      <a class="control" href={serviceHref(path, detail.version, scope, { state: '' })}
        >Retry</a
      >
    {/if}
  </div>
  <div class="channels">
    {#each view.channels as channel (channel.label)}
      <span class="channel"><span class="k">{channel.label}</span><span class="v">{channel.value}</span></span>
    {/each}
  </div>
  <p class="mono muted">{view.channelNote}</p>
</section>

<p class="mono muted">
  range applies to the operational charts — drift and eval signals keep their own windows
</p>
<div class="dashboard" aria-label="Operating dashboard">
  {#each view.charts as chart (chart.title)}
    {@const window = windowFor(chart)}
    <ChartPanel
      title={chart.title}
      measure={chart.measure}
      unit={chart.unit}
      latestValue={chart.latest}
      source={chart.source}
      from={window.from}
      to={window.to}
      link={{ label: chart.link.label, href: base + observeHref(chart.link.href, scope.range) }}
      state={panelStates[chart.state]}
      detail={chart.state === 'gap' && chart.latest ? `${chart.latest} · ${chart.note}` : chart.note}
      code={chart.code}
    >
      {#if chart.series.length}
        <Line
          series={chart.series}
          labels={pointLabels(chart)}
          min={chart.min}
          threshold={chart.threshold}
        />
      {/if}
    </ChartPanel>
  {/each}
  <StateBlock
    state="absent"
    title={view.addChart.title}
    detail={view.addChart.lines.join(' ')}
  />
</div>

<div class="signals" aria-label="Published intelligence">
  {#each view.signals as signal (signal.title)}
    <Panel title={signal.title} variant="quiet">
      {#snippet head()}{#if signal.stamp}<span class="mono muted">{signal.stamp}</span>{/if}{/snippet}
      {#if signal.state === 'unauthorized'}
        <StateBlock state="unauthorized" title="NOT AUTHORIZED" detail={signal.body?.[0]} />
        {#each signal.body?.slice(1) ?? [] as line (line)}
          <p class="mono muted">{line}</p>
        {/each}
      {:else if signal.state === 'nodata'}
        <StateBlock state="absent" title="No result at this scope" detail={signal.body?.[0]} />
      {:else}
        <div class="sigverdict">
          {#if signal.verdict}<Badge tone={signal.verdict.tone}>{signal.verdict.label}</Badge>{/if}
          {#if signal.subject}<span class="mono muted">{signal.subject}</span>{/if}
        </div>
        {#if signal.chart}
          <Line
            series={[{ label: signal.chart.latest, points: signal.chart.points }]}
            min={signal.chart.min}
            threshold={signal.chart.threshold}
          />
          <p class="mono muted sigaxis">
            <span>{signal.chart.axis[0]}</span><span>{signal.chart.axis[1]}</span>
          </p>
          <p class="siglatest">
            <span class="lv">{signal.chart.latest}</span>
            <span class="mono muted">{signal.chart.qualifier}</span>
          </p>
        {/if}
        {#if signal.context}<p class="mono muted">{signal.context}</p>{/if}
        {#if signal.link}
          <a class="mono" href={base + observeHref(signal.link.href, scope.range)}>{signal.link.label}</a>
        {/if}
      {/if}
    </Panel>
  {/each}
</div>

<Panel title={view.attention.title} variant="quiet">
  {#snippet head()}<span class="mono muted">{view.attention.note}</span>{/snippet}
  {#if view.attention.item}
    <div class="attention-item">
      <span class="mono"><strong>{view.attention.item.text}</strong></span>
      <span class="mono muted">{view.attention.item.sub}</span>
      <a class="mono" href={base + view.attention.item.href}>{view.attention.item.label}</a>
    </div>
  {/if}
  {#each view.attention.lines as line (line)}
    <p class="muted">{line}</p>
  {/each}
</Panel>

<Panel title="OPERATIONAL CONTEXT" variant="quiet">
  {#snippet head()}<span class="mono muted">each keeps its route</span>{/snippet}
  <div class="kv">
    <span class="k">components</span>
    <span class="mono">
      {view.context.components.text}
      {#if view.context.components.href}
        · <a href={contextHref(view.context.components.href)} data-sveltekit-noscroll>→</a>
      {/if}
    </span>
    <span class="k">activity</span>
    <span class="mono">{view.context.activity}</span>
  </div>
  <div class="investigate">
    <span class="mono muted">investigate</span>
    {#each view.context.investigations as item (item.label)}
      {#if item.href}
        <a href={base + observeHref(item.href, scope.range)}>{item.label}</a>
      {:else}
        <span class="mono muted">{item.label} {item.note}</span>
      {/if}
    {/each}
    <span class="mono muted">· {view.context.foot}</span>
  </div>
</Panel>
