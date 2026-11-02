<!--
  The open record of the Observe view: a trace's waterfall and selected span
  in the drawer, or a log record beside the list.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { Drawer, EmptyState, RecordDetail, TraceWaterfall, type ActionIntent, type ActionState } from 'wyrd-ai-ui';
  import { validateIntent, type ComponentName } from 'wyrd-ai-ui/loaders';
  import { closed, readState, step, writeState, type ObserveState } from '$lib/observe-host';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const current = $derived(readState(page.url.searchParams));
  const go = (next: ObserveState) => goto(`${base}/observe?${writeState(next)}`, { keepFocus: true, noScroll: true });

  let actions = $state<Record<string, ActionState>>({});
  function onaction(component: ComponentName) {
    return (intent: ActionIntent) => {
      const valid = validateIntent(component, intent);
      if ('code' in valid) return void (actions[intent.name] = { state: 'error', problem: valid });
      const next = step(current, valid, `${page.url.pathname}${page.url.search}`);
      if ('unavailable' in next) return void (actions[valid.name] = { state: 'unavailable', reason: next.unavailable });
      delete actions[valid.name];
      if ('query' in next) return void goto(`${base}/query?${next.query}`);
      void go(next.state);
    };
  }
</script>

{#if data.waterfall}
  <Drawer open title="Trace" onclose={() => go(closed(current))}>
    {#if 'code' in data.waterfall}<EmptyState state="error" problem={data.waterfall} />{:else}<TraceWaterfall {...data.waterfall} {actions} onaction={onaction('TraceWaterfall')} />{/if}
    {@render record()}
  </Drawer>
{:else if data.record}
  <aside class="record" aria-label="Log record">
    <header>
      <p>Log record</p>
      <a href={`${base}/observe?${writeState(closed(current))}`} aria-label="Close log record" data-sveltekit-noscroll>✕</a>
    </header>
    {@render record()}
  </aside>
{/if}

{#snippet record()}
  {#if data.record && 'code' in data.record}<EmptyState state="error" problem={data.record} />{:else if data.record}<RecordDetail {...data.record} {actions} onaction={onaction('RecordDetail')} />{/if}
{/snippet}

<style>
  .record {
    max-width: 640px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  header {
    display: flex;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  header p {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  header a {
    color: var(--muted);
    text-decoration: none;
  }
</style>
