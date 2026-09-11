<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import Select from '$lib/components/Select.svelte';
  import { RANGES } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/evaluations$/, ''));
  const subjectOptions = $derived([
    { value: '', label: 'all subjects' },
    ...(data.view?.subjects ?? []).map((s: string) => ({ value: s, label: s }))
  ]);
  const statusOptions = [
    { value: '', label: 'any status' },
    { value: 'passed', label: 'passed' },
    { value: 'failed', label: 'failed' },
    { value: 'running', label: 'running' }
  ];
  const originOptions = [
    { value: '', label: 'any origin' },
    { value: 'online', label: 'online' },
    { value: 'offline', label: 'offline' }
  ];
  const rangeOptions = RANGES.map((r) => ({ value: r, label: r }));
</script>

<svelte:head><title>Evaluations · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Evaluations</h1>
  <p class="muted">Evaluation events, keyed by record_id — run_id is correlation, not identity.</p>
  <ObserveNav base={`${base}/observe`} current="Evaluations" scope={data.scope} />
  <div class="stack">
    <form class="filters row" method="GET" data-sveltekit-noscroll data-sveltekit-keepfocus>
      <input aria-label="Search evaluation events" name="q" value={data.filters.q} placeholder="Search record_id…" />
      <button class="control">Search</button>
      <Select label="subject" name="service" options={subjectOptions} value={data.filters.service} />
      <Select label="status" name="status" options={statusOptions} value={data.filters.status} />
      <Select label="origin" name="origin" options={originOptions} value={data.filters.origin} />
      <Select label="range" name="range" options={rangeOptions} value={data.filters.range} />
      <a class="mono muted reset" href={page.url.pathname}>Reset</a>
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
        {#if !data.view.rows.length}
          <StateBlock
            state="empty"
            title="No matching evaluation events"
            detail="Your search and filters are preserved."
            actionLabel="Clear search and filters"
            actionHref={page.url.pathname}
          />
        {:else}
          <Panel title="Events" variant="raised">
            {#snippet head()}<span class="mono muted">{data.view.rows.length} of {data.view.matching} · keyed by record_id</span>{/snippet}
            <Table label="Evaluation events">
              <table>
                <thead><tr><th>Record_id</th><th>Eval</th><th>Subject</th><th>Run_id</th><th>Origin</th><th>Status</th><th>Tasks</th><th>Dur</th></tr></thead>
                <tbody>
                  {#each data.view.rows as row (row.recordId)}
                    <tr>
                      <td class="mono">
                        <a href={`${page.url.pathname}/${row.recordId}${data.filters.service ? `?service=${encodeURIComponent(data.filters.service)}` : ''}`}><strong>{row.recordId}</strong></a>
                        {#if row.scenario}
                          <p class="muted">scenario {row.scenario.id} · {row.scenario.collection}</p>
                        {/if}
                      </td>
                      <td>{row.eval}</td>
                      <td class="mono">{row.subject}</td>
                      <td class="mono">{row.runId}</td>
                      <td><Badge tone={row.origin === 'offline' ? 'neutral' : 'running'}>{row.origin}</Badge></td>
                      <td><Badge tone={row.status.tone}>{row.status.label}</Badge></td>
                      <td class="mono">{row.passed}/{row.total}</td>
                      <td class="mono">{row.duration}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </Table>
            <p class="muted">
              record_id is the event's identity; run_id correlates it with traces, logs and metrics
              from the same run. Pasting this URL re-renders the same chips and result set.
            </p>
          </Panel>
        {/if}
        <div class="stack">
          <Panel title="Filters">
            {#snippet head()}<span class="mono muted">URL-backed</span>{/snippet}
            <div class="kv">
              <span class="k">evalCard</span><span class="mono">{data.filters.evalCard || 'any'}</span>
              <span class="k">service</span><span class="mono">{data.filters.service || 'any'}</span>
              <span class="k">origin</span><span class="mono">{data.filters.origin || 'any'}</span>
              <span class="k">status</span><span class="mono">{data.filters.status || 'any'}</span>
              <span class="k">range</span><span class="mono">{data.filters.range}</span>
            </div>
          </Panel>
          <Panel title="Online and offline">
            <p class="muted">
              <strong class="mono">online</strong> events are emitted by registered agents running in
              production; the server loads the bound workflow, scores the record and stores the
              result.
            </p>
            <p class="muted">
              <strong class="mono">offline</strong> events come from local scenario runs — an agent
              that may not be registered yet. They carry scenario and collection identity instead of
              a registered subject.
            </p>
          </Panel>
        </div>
      </div>
    {/if}
  </div>
</div>
