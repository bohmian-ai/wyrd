<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import SpanFlow from '$lib/features/observe/SpanFlow.svelte';
  import { goto } from '$app/navigation';
  import { withScope } from '$lib/features/observe/core/filter-state';
  import { serviceColorMap } from '$lib/features/observe/core/service-color';
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
  function viewHref(view: 'waterfall' | 'flow' | 'genai'): string {
    const query = new URLSearchParams(page.url.search);
    if (view === 'waterfall') query.delete('view');
    else query.set('view', view);
    const search = `${query}`;
    return search ? `?${search}` : page.url.pathname;
  }
  const activeView = $derived.by(() => {
    const view = page.url.searchParams.get('view');
    return view === 'flow' || view === 'genai' ? view : 'waterfall';
  });
  /** Span detail tab links keep the selected span and the trace view. */
  function tabHref(tab: string): string {
    const query = new URLSearchParams(page.url.search);
    if (tab === 'attributes') query.delete('tab');
    else query.set('tab', tab);
    return `?${query}`;
  }
  const activeTab = $derived.by(() => {
    const tab = page.url.searchParams.get('tab') ?? 'attributes';
    if (tab === 'genai' && !data.view?.selected.genai) return 'attributes';
    return ['attributes', 'events', 'links', 'genai'].includes(tab) ? tab : 'attributes';
  });
  /** Spans with an extracted genai.* record, in waterfall (start) order. */
  const genaiSpans = $derived(data.view?.spans.filter((span) => span.genai) ?? []);
  /** Trace-level GenAI rollup — tokens, models and errors over the extracted records. */
  const genaiRollup = $derived.by(() => {
    let input = 0;
    let output = 0;
    const models = new Set<string>();
    let errors = 0;
    for (const span of genaiSpans) {
      const record = span.genai!;
      if (record.table === 'messages') {
        input += record.usage.input;
        output += record.usage.output;
        models.add(record.requestModel);
      } else {
        models.add(record.toolName);
      }
      if (record.errorType) errors += 1;
    }
    return { input, output, models: [...models], errors };
  });
  const colors = $derived(serviceColorMap(data.view?.spans.map((span) => span.service) ?? []));
  /** Trace duration in seconds, parsed once for the timeline tick labels. */
  const durationS = $derived(parseFloat(data.view?.duration ?? '0') || 0);
  const ticks = $derived(
    [0, 0.25, 0.5, 0.75, 1].map((f) => ({ pct: f * 100, label: `${(durationS * f).toFixed(2)}s` }))
  );
  /** A flow node click selects that span — same URL state as a waterfall row. */
  function selectSpan(id: string) {
    goto(spanHref(id), { noScroll: true, keepFocus: true });
  }
</script>

{#snippet genaiBody(record: import('$lib/features/observe/core/types').GenAiSpan)}
  {#if record.table === 'messages'}
    <div class="kv">
      <span class="k">provider</span><span class="mono">{record.provider} · {record.operation}</span>
      <span class="k">model</span><span class="mono">{record.requestModel}{record.responseModel && record.responseModel !== record.requestModel ? ` → ${record.responseModel}` : ''}</span>
      {#if record.conversationId}<span class="k">conversation</span><span class="mono">{record.conversationId}</span>{/if}
      {#each record.params as [key, value] (key)}
        <span class="k">{key}</span><span class="mono">{value}</span>
      {/each}
      <span class="k">usage</span><span class="mono">{record.usage.input.toLocaleString()} in · {record.usage.output.toLocaleString()} out{record.usage.cacheRead ? ` · ${record.usage.cacheRead.toLocaleString()} cache read` : ''}{record.usage.cacheCreate ? ` · ${record.usage.cacheCreate.toLocaleString()} cache create` : ''}</span>
      {#if record.finishReasons.length}<span class="k">finish</span><span class="mono">{record.finishReasons.join(', ')}</span>{/if}
    </div>
    {#if record.withheldReason}
      <p class="muted">{record.withheldReason}</p>
    {:else}
      {#if record.systemInstructions}
        <div class="msg"><p class="mono role">system</p><p>{record.systemInstructions}</p></div>
      {/if}
      {#each record.inputMessages ?? [] as message, i (i)}
        <div class="msg"><p class="mono role">{message.role}</p><p>{message.content}</p></div>
      {/each}
      {#each record.outputMessages ?? [] as message, i (i)}
        <div class="msg out"><p class="mono role">{message.role}</p><p>{message.content}</p></div>
      {/each}
    {/if}
  {:else}
    <div class="kv">
      <span class="k">provider</span><span class="mono">{record.provider} · {record.operation}</span>
      <span class="k">tool</span><span class="mono">{record.toolName} · {record.toolType}</span>
      {#if record.conversationId}<span class="k">conversation</span><span class="mono">{record.conversationId}</span>{/if}
      {#if record.errorType}<span class="k">error_type</span><span class="mono error">{record.errorType}</span>{/if}
    </div>
    {#if record.withheldReason}
      <p class="muted">{record.withheldReason}</p>
    {:else}
      {#if record.args}
        <p class="mono role">arguments</p>
        <pre class="code mono">{record.args}</pre>
      {/if}
      {#if record.result}
        <p class="mono role">result</p>
        <pre class="code mono">{record.result}</pre>
      {:else if record.errorType}
        <p class="muted">No result — the call failed with {record.errorType}.</p>
      {/if}
    {/if}
  {/if}
{/snippet}

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
    {@const selected = data.view.selected}
    <h1>{data.view.rootOperation}</h1>
    <p class="muted">One trace: waterfall, span flow and the selected span's detail.</p>
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
              <span class="k">logs</span><a class="mono" href={withScope(`${base}/observe/logs`, data.scope, { trace: data.view.id })}>→ logs for {data.view.id}</a>
            </div>
          </Panel>
          <div class="row view-tabs" role="tablist" aria-label="Trace views">
            <a class="control" href={viewHref('waterfall')} data-sveltekit-noscroll role="tab" aria-selected={activeView === 'waterfall'}>Waterfall</a>
            <a class="control" href={viewHref('flow')} data-sveltekit-noscroll role="tab" aria-selected={activeView === 'flow'}>Flow</a>
            {#if genaiSpans.length}
              <a class="control" href={viewHref('genai')} data-sveltekit-noscroll role="tab" aria-selected={activeView === 'genai'}>GenAI · {genaiSpans.length}</a>
            {/if}
            <span class="mono muted legend">
              {#each [...colors] as [service, color] (service)}
                <span class="swatch"><i style={`background:${color}`}></i>{service}</span>
              {/each}
            </span>
          </div>
          {#each data.view.graph.edges.filter((edge) => edge.error) as edge (edge.from)}
            <p class="error mono">✕ error edge: {edge.from} → {edge.to} ({edge.label})</p>
          {/each}
          {#if activeView === 'waterfall'}
            <Panel title="Waterfall" variant="raised">
              {#snippet head()}<span class="mono muted">all {data.view.spanCount} spans · scrolls</span>{/snippet}
              <div class="timeline mono muted" aria-hidden="true">
                <span class="tl-label">operation</span>
                <span class="tl-lane">
                  {#each ticks as tick (tick.pct)}
                    <span class="tl-tick" style={`left:${tick.pct}%`}>{tick.label}</span>
                  {/each}
                </span>
                <span></span>
              </div>
              <div class="waterfall-scroll">
                <div class="waterfall">
                  {#each data.view.spans as span (span.id)}
                    <a
                      class="span-row"
                      href={spanHref(span.id)} data-sveltekit-noscroll
                      aria-current={data.view.selected.id === span.id ? 'true' : undefined}
                    >
                      <span class="op" style={`padding-left:${span.depth * 14}px`}>
                        {#if span.depth > 0}<span class="conn" aria-hidden="true"></span>{/if}
                        <i class="svc-dot" style={`background:${colors.get(span.service)}`} aria-hidden="true"></i>
                        <strong>{span.name}</strong>
                        <small class="muted"> {span.service} · {span.kind}</small>
                        {#if span.genai}<em class="genai-chip">GENAI</em>{/if}
                      </span>
                      <span class="lane">
                        <span
                          class={`bar ${span.error ? 'err' : ''}`}
                          style={`left:${span.startPct}%;width:${span.widthPct}%;${span.error ? '' : `background:${colors.get(span.service)}`}`}
                        ></span>
                      </span>
                      <span class={`dur ${span.error ? 'err' : ''}`}>{span.durationLabel}</span>
                    </a>
                  {/each}
                </div>
              </div>
              <p class="muted">The waterfall scrolls rather than compressing — no span row is dropped.</p>
            </Panel>
          {:else if activeView === 'flow'}
            <Panel title="Flow" variant="raised">
              {#snippet head()}<span class="mono muted">span DAG · drag, zoom, click a node to inspect it</span>{/snippet}
              <SpanFlow
                spans={data.view.spans}
                {colors}
                selectedId={data.view.selected.id}
                onselect={selectSpan}
              />
              <p class="muted">Every drawn span is a node; a click selects it in the span detail panel.</p>
            </Panel>
          {:else}
            <Panel title="GenAI" variant="raised">
              {#snippet head()}<span class="mono muted">{genaiSpans.length} genai spans · {genaiRollup.input.toLocaleString()} in / {genaiRollup.output.toLocaleString()} out tokens · {genaiRollup.models.join(', ')} · {genaiRollup.errors} error</span>{/snippet}
              {#if !genaiSpans.length}
                <StateBlock state="empty" title="No extracted GenAI records" detail="This trace has no genai.messages or genai.tool_calls rows. Absence is normal for non-agent traffic." />
              {:else}
                <div class="stack genai-timeline">
                  {#each genaiSpans as span (span.id)}
                    {@const record = span.genai!}
                    <section class="genai-block">
                      <p class="mono genai-head">
                        <a href={spanHref(span.id)} data-sveltekit-noscroll><strong>{span.name}</strong></a>
                        · {record.table === 'messages' ? record.requestModel : record.toolName}
                        · {span.durationLabel}
                        <Badge tone={record.errorType ? 'danger' : 'ok'}>{record.errorType ?? 'OK'}</Badge>
                      </p>
                      {@render genaiBody(record)}
                    </section>
                  {/each}
                </div>
              {/if}
            </Panel>
          {/if}
        </div>
        <div class="stack">
          <Panel title="Selected span" variant="raised">
            {#snippet head()}<span class="mono muted">{selected.name}</span>{/snippet}
            <div class="kv">
              <span class="k">status</span><span><Badge tone={selected.error ? 'danger' : 'ok'}>{selected.status}</Badge></span>
              <span class="k">kind</span><span class="mono">{selected.kind}</span>
              <span class="k">service</span><span class="mono">{selected.service}</span>
              <span class="k">scope</span><span class="mono">{selected.scopeName} @ {selected.scopeVersion}</span>
              <span class="k">span_id</span><span class="mono">{selected.id}</span>
              <span class="k">parent</span><span class="mono">{selected.parentSpanId ?? '— root'}</span>
              <span class="k">duration</span><span class="mono">{selected.durationLabel}</span>
            </div>
            <div class="row span-tabs" role="tablist" aria-label="Span detail sections">
              <a class="control" href={tabHref('attributes')} data-sveltekit-noscroll role="tab" aria-selected={activeTab === 'attributes'}>Attributes · {Object.keys(selected.attributes).length}</a>
              <a class="control" href={tabHref('events')} data-sveltekit-noscroll role="tab" aria-selected={activeTab === 'events'}>Events · {selected.events.length}</a>
              <a class="control" href={tabHref('links')} data-sveltekit-noscroll role="tab" aria-selected={activeTab === 'links'}>Links · {selected.links.length}</a>
              {#if selected.genai}
                <a class="control" href={tabHref('genai')} data-sveltekit-noscroll role="tab" aria-selected={activeTab === 'genai'}>GenAI</a>
              {/if}
            </div>
            {#if activeTab === 'attributes'}
              {#if Object.keys(selected.attributes).length}
                <div class="kv">
                  {#each Object.entries(selected.attributes) as [key, value] (key)}
                    <span class="k">{key}</span><span class="mono">{value}</span>
                  {/each}
                </div>
              {:else}
                <p class="muted">No attributes on this span.</p>
              {/if}
              {#if selected.droppedAttributesCount > 0}
                <p class="muted">{selected.droppedAttributesCount} attributes dropped at the source.</p>
              {/if}
            {:else if activeTab === 'events'}
              {#if selected.events.length}
                <div class="stack events">
                  {#each selected.events as event, i (event.name + i)}
                    <section class={`event ${event.name === 'exception' ? 'err' : ''}`}>
                      <p class="mono event-head"><strong>{event.name}</strong> <span class="muted">{event.offset}</span></p>
                      <div class="kv">
                        {#each Object.entries(event.attributes).filter(([key]) => key !== 'exception.stacktrace') as [key, value] (key)}
                          <span class="k">{key}</span><span class="mono">{value}</span>
                        {/each}
                      </div>
                      {#if typeof event.attributes['exception.stacktrace'] === 'string'}
                        <pre class="code mono">{event.attributes['exception.stacktrace']}</pre>
                      {/if}
                    </section>
                  {/each}
                </div>
              {:else}
                <p class="muted">0 events recorded on this span.</p>
              {/if}
            {:else if activeTab === 'links'}
              {#if selected.links.length}
                <div class="stack">
                  {#each selected.links as link (link.linkedTraceId + link.linkedSpanId)}
                    <p class="mono">
                      <a href={withScope(`${base}/observe/traces/${link.linkedTraceId}`, data.scope, { span: link.linkedSpanId })}>→ {link.linkedTraceId} · {link.linkedSpanId}</a>
                      {#each Object.entries(link.attributes) as [key, value] (key)}
                        <span class="muted"> · {key}={value}</span>
                      {/each}
                    </p>
                  {/each}
                </div>
              {:else}
                <p class="muted">0 links recorded on this span.</p>
              {/if}
            {:else if selected.genai}
              {@render genaiBody(selected.genai)}
            {/if}
          </Panel>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .back {
    font-weight: 700;
  }
  .genai-chip {
    font: 700 8px var(--font-mono);
    font-style: normal;
    letter-spacing: 0.06em;
    padding: 1px 4px;
    margin-left: 6px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--brand-soft);
    vertical-align: 1px;
  }
  .span-tabs {
    gap: 6px;
    margin: 12px 0 10px;
    padding-top: 10px;
    border-top: 2px dashed var(--border);
    flex-wrap: wrap;
    font-size: 11px;
  }
  .span-tabs .control[aria-selected='true'] {
    background: var(--brand-soft);
    box-shadow: 2px 2px 0 0 var(--shadow);
  }
  .events {
    gap: 10px;
  }
  .event {
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 8px 10px;
  }
  .event.err {
    border-color: var(--danger);
  }
  .event-head {
    margin: 0 0 6px;
    font-size: 11px;
  }
  .code {
    margin: 4px 0 0;
    padding: 8px 10px;
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
  }
  .role {
    margin: 10px 0 2px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .msg {
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 6px 10px 8px;
    margin-top: 8px;
  }
  .msg .role {
    margin: 0 0 4px;
  }
  .msg p:not(.role) {
    margin: 0;
  }
  .msg.out {
    background: var(--brand-soft);
  }
  .genai-timeline {
    gap: 14px;
  }
  .genai-block {
    border-top: 2px dashed var(--border);
    padding-top: 10px;
  }
  .genai-block:first-child {
    border-top: 0;
    padding-top: 0;
  }
  .genai-head {
    margin: 0 0 6px;
    font-size: 11px;
  }
  .error {
    color: var(--danger);
    font-weight: 700;
  }
  .trace-facts {
    grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr);
  }
  .view-tabs {
    align-items: center;
    gap: 8px;
  }
  .view-tabs .control[aria-selected='true'] {
    background: var(--brand-soft);
    box-shadow: 2px 2px 0 0 var(--shadow);
  }
  .legend {
    margin-left: auto;
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    font-size: 10px;
  }
  .swatch {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .swatch i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .timeline {
    display: grid;
    grid-template-columns: minmax(150px, 220px) minmax(0, 1fr) 60px;
    gap: 10px;
    font-size: 9px;
    padding-bottom: 14px;
    border-bottom: 2px dashed var(--border);
    margin-bottom: 4px;
  }
  .tl-label {
    text-transform: uppercase;
    letter-spacing: 0.5px;
    font-weight: 700;
  }
  .tl-lane {
    position: relative;
  }
  .tl-tick {
    position: absolute;
    transform: translateX(-50%);
    border-left: 1px solid var(--border);
    padding: 0 3px 2px;
    white-space: nowrap;
  }
  .tl-tick:first-child {
    transform: none;
  }
  .tl-tick:last-child {
    transform: translateX(-100%);
    border-left: 0;
    border-right: 1px solid var(--border);
  }
  .op {
    position: relative;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .conn {
    position: absolute;
    left: 0;
    top: 50%;
    width: 9px;
    border-top: 1px solid var(--border);
    margin-left: -11px;
  }
  .svc-dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    border-radius: 2px;
    margin-right: 6px;
    vertical-align: baseline;
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
    .timeline {
      grid-template-columns: minmax(110px, 150px) minmax(0, 1fr) 50px;
      gap: 6px;
    }
  }
</style>
