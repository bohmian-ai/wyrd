<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import { scopeQuery, withScope } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/traces\/[^/]+$/, ''));
  const backHref = $derived(
    withScope(`${base}/observe/traces`, data.scope, { status: data.back.status })
  );
  const backSearch = $derived(
    [
      data.scope.service && `service=${data.scope.service}`,
      data.back.status && `status=${data.back.status}`,
      data.scope.range
    ]
      .filter(Boolean)
      .join(' · ')
  );
  function spanHref(id: string): string {
    const query = new URLSearchParams(page.url.search);
    query.set('span', id);
    return `?${query}`;
  }
</script>

<svelte:head><title>{data.view?.id ?? 'Trace'} · Observe · Wyrd</title></svelte:head>
<div class="observe">
  {#if data.problem}
    <h1>Trace</h1>
    <ObserveNav base={`${base}/observe`} current="Traces" scope={data.scope} />
    <StateBlock
      state={data.problem.status === 403 ? 'unauthorized' : data.problem.status === 404 ? 'absent' : 'error'}
      title={data.problem.title}
      code={data.problem.code}
      actionLabel="Back to traces"
      actionHref={backHref}
    />
  {:else if data.view}
    <h1>{data.view.rootOperation}</h1>
    <p class="muted">One trace: waterfall, service graph and the selected span's detail.</p>
    <ObserveNav base={`${base}/observe`} current="Traces" scope={data.scope} />
    <div class="stack">
      <a class="mono back" href={backHref}>← Back to traces — the search ({backSearch}) is restored intact</a>
      <div class="columns">
        <div class="stack">
          <Panel>
            <div class="kv trace-facts">
              <span class="k">trace id</span><span class="mono"><strong>{data.view.id}</strong></span>
              <span class="k">root</span><span class="mono">{data.view.rootOperation}</span>
              <span class="k">service</span><span class="mono">{data.view.service}</span>
              <span class="k">start</span><span class="mono">{data.view.start}</span>
              <span class="k">duration</span><span class="mono"><strong>{data.view.duration}</strong> <Badge tone="danger">Error</Badge></span>
              <span class="k">spans</span><span class="mono">{data.view.spanCount} spans · {data.view.errorCount} error</span>
            </div>
          </Panel>
          <Panel title="Waterfall" variant="raised">
            {#snippet head()}<span class="mono muted">scrolls · {data.view.spans.length} of {data.view.spanCount} rows shown</span>{/snippet}
            <div class="waterfall-scroll">
              <div class="waterfall">
                {#each data.view.spans as span (span.id)}
                  <a
                    class="span-row"
                    href={spanHref(span.id)}
                    aria-current={data.view.selected.id === span.id ? 'true' : undefined}
                  >
                    <span><strong>{span.name}</strong><br /><small class="muted">{span.kind}</small></span>
                    <span class="lane"><span class={`bar ${span.error ? 'err' : ''}`} style={`left:${span.startPct}%;width:${span.widthPct}%`}></span></span>
                    <span class={`dur ${span.error ? 'err' : ''}`}>{span.durationLabel}</span>
                  </a>
                {/each}
              </div>
            </div>
            <p class="muted">The waterfall scrolls rather than compressing — no span row is dropped.</p>
          </Panel>
          <Panel title="Service graph">
            {#snippet head()}<span class="mono muted">error edge highlighted</span>{/snippet}
            <div class="svc-graph">
              {#each data.view.graph.nodes as node, i (node.name)}
                {#if i > 0}
                  <span class={`edge ${data.view.graph.edges[i - 1]?.error ? 'err' : ''}`} aria-hidden="true"></span>
                {/if}
                <a class="node" href={`${base}/cards/${node.cardHref}`}>{node.name}</a>
              {/each}
            </div>
            {#each data.view.graph.edges.filter((edge) => edge.error) as edge (edge.from)}
              <p class="error mono">✕ error edge: {edge.from} → {edge.to} ({edge.label})</p>
            {/each}
          </Panel>
        </div>
        <div class="stack">
          <Panel title="Selected span" variant="raised">
            {#snippet head()}<span class="mono muted">{data.view.selected.name}</span>{/snippet}
            <div class="kv">
              {#each data.view.selected.fields as [key, value], i (key + i)}
                <span class="k">{key}</span><span class="mono">{value}</span>
              {/each}
              {#if data.view.selected.link}
                <span class="k">link</span>
                <a class="mono" href={withScope(`${base}/observe/traces/${data.view.selected.link.href}`, data.scope)}>{data.view.selected.link.label}</a>
              {/if}
            </div>
          </Panel>
          {#if data.view.selected.aiContent}
            <Panel title="AI content">
              <div class="kv">
                <span class="k">prompt</span><span>{data.view.selected.aiContent.prompt}</span>
                <span class="k">completion</span><span>{data.view.selected.aiContent.completion}</span>
              </div>
            </Panel>
          {:else if data.view.selected.aiAbsentReason}
            <StateBlock state="absent" title="AI content" detail={data.view.selected.aiAbsentReason} />
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .back {
    font-weight: 700;
  }
  .trace-facts {
    grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr);
  }
  .waterfall-scroll {
    max-height: 420px;
    overflow-y: auto;
  }
  @media (max-width: 700px) {
    .trace-facts {
      grid-template-columns: auto minmax(0, 1fr);
    }
    .waterfall-scroll {
      max-height: 320px;
    }
  }
</style>
