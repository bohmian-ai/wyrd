<script lang="ts">
  import type { Snippet } from 'svelte';
  import Panel from '../Panel.svelte';
  import StateBlock from '../StateBlock.svelte';

  // The one frame every Wyrd chart is drawn in. It carries what a reader needs before
  // trusting a plot — what is measured, in what unit, over which range, from which
  // source, how fresh it is, and where the canonical view lives — and it owns the
  // unavailable states. A chart that cannot be drawn says why: no latest value is shown
  // for a loading, empty, unauthorized or failed measure, because a stale or absent
  // reading rendered as a number is a health claim the server never made.
  type State = 'ok' | 'loading' | 'empty' | 'partial' | 'unauthorized' | 'error';
  type Stamp = { label: string; at?: string };
  let {
    title,
    measure,
    unit,
    latestValue,
    source,
    freshness,
    from,
    to,
    link,
    state = 'ok',
    detail,
    code,
    children
  }: {
    title: string;
    measure: string;
    unit?: string;
    latestValue?: string | number;
    source?: string;
    freshness?: Stamp;
    from?: Stamp;
    to?: Stamp;
    link?: { label: string; href: string };
    state?: State;
    detail?: string;
    code?: string;
    children?: Snippet;
  } = $props();

  const drawable = $derived(state === 'ok' || state === 'partial');

  const stateTitles = $derived<Record<Exclude<State, 'ok'>, string>>({
    loading: `Loading ${measure}`,
    empty: `No ${measure} in this range`,
    partial: `Partial ${measure}`,
    unauthorized: `Not authorized to read ${measure}`,
    error: `${measure} could not be loaded`
  });
</script>

<Panel {title} variant="quiet">
  {#snippet head()}
    <span class="meas">{measure}{#if unit}<span class="unit">&nbsp;· {unit}</span>{/if}</span>
    {#if link}<a class="lnk" href={link.href}>{link.label}</a>{/if}
  {/snippet}

  <div class="wy-chartpanel">
    {#if state === 'ok' && latestValue !== undefined}
      <p class="latest">
        <span class="lv">{latestValue}</span>
        {#if unit}<span class="lu">{unit}</span>{/if}
        {#if freshness}
          <span class="fr">
            {#if freshness.at}<time datetime={freshness.at}>{freshness.label}</time>{:else}{freshness.label}{/if}
          </span>
        {/if}
      </p>
    {/if}

    {#if state !== 'ok'}
      <StateBlock state={state === 'partial' ? 'partial' : state} title={stateTitles[state]} {detail} {code} />
    {/if}

    {#if drawable}
      <div class="plot">{@render children?.()}</div>
    {/if}

    <p class="foot">
      {#if from && to}
        <span class="rng">
          {#if from.at}<time datetime={from.at}>{from.label}</time>{:else}{from.label}{/if}
          <span aria-hidden="true"> → </span>
          {#if to.at}<time datetime={to.at}>{to.label}</time>{:else}{to.label}{/if}
        </span>
      {/if}
      {#if source}<span class="src">{source}</span>{/if}
    </p>
  </div>
</Panel>

<style>
  .meas {
    font-family: var(--fm);
  }
  .unit {
    color: var(--muted);
    font-weight: 400;
  }
  .lnk {
    font-family: var(--fm);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--brand-strong);
  }
  .latest {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 0 8px;
  }
  .lv {
    font-family: var(--font-display);
    font-size: 22px;
    font-weight: 700;
    color: var(--text);
  }
  .lu,
  .fr {
    font-family: var(--fm);
    font-size: 10px;
    color: var(--muted);
  }
  .fr {
    margin-left: auto;
  }
  .plot {
    min-width: 0;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 9px 0 0;
    padding-top: 8px;
    border-top: 2px dashed var(--border);
    font-family: var(--fm);
    font-size: 9px;
    color: var(--muted);
  }
  .src {
    margin-left: auto;
  }
</style>
