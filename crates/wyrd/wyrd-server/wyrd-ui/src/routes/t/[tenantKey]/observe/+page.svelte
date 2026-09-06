<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Badge from '$lib/components/Badge.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import ScopeChips from '$lib/features/observe/ScopeChips.svelte';
  import { filterChips } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const chips = $derived(
    filterChips(page.url.pathname, { service: data.scope.service, range: data.scope.range })
  );
</script>

<svelte:head><title>Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Observe</h1>
  <p class="muted">Operational health and the entry point for every investigation.</p>
  <ObserveNav base={page.url.pathname} current="Overview" scope={data.scope} />
  <div class="stack">
    <ScopeChips
      {chips}
      clearHref={page.url.pathname}
      note="Attention and activity are tenant-wide; the chips scope the signal tiles and signal pages."
    />
    {#if data.problem}
      <StateBlock
        state={data.problem.status === 403 ? 'unauthorized' : 'error'}
        title={data.problem.title}
        code={data.problem.code}
        actionLabel="Retry"
        actionHref={page.url.pathname + page.url.search}
      />
    {:else if data.view}
      <div class="tiles">
        {#each data.view.tiles as tile (tile.signal)}
          <Panel title={tile.signal} variant={tile.noData ? 'flat' : 'quiet'}>
            {#snippet head()}<span class="mono muted">{data.scope.range}</span>{/snippet}
            <p class="tile-value">{tile.value}{#if tile.unit}<span class="tile-unit"> {tile.unit}</span>{/if}</p>
            <p><Badge tone={tile.status.tone}>{tile.status.label}</Badge></p>
            <a class="mono" href={tile.href}>→ {tile.signal}</a>
          </Panel>
        {/each}
      </div>
      <div class="columns">
        <div class="stack">
          <Panel title="Attention" variant="raised">
            {#snippet head()}<span class="mono muted">tenant-wide · {data.view.attention.length} items</span>{/snippet}
            <Table label="Attention">
              <table>
                <thead><tr><th>Signal</th><th>What</th><th>Service</th><th>State</th><th>Since</th></tr></thead>
                <tbody>
                  {#each data.view.attention as row (row.what)}
                    <tr>
                      <td><a href={row.href}><strong>{row.signal}</strong></a></td>
                      <td>{row.what}</td>
                      <td class="mono">{row.service}</td>
                      <td><Badge tone={row.state.tone}>{row.state.label}</Badge></td>
                      <td>{row.since}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </Table>
          </Panel>
          <Panel title="Recent observation activity">
            {#snippet head()}<span class="mono muted">tenant-wide · latest {data.view.recent.length}</span>{/snippet}
            <Table label="Recent observation activity">
              <table>
                <thead><tr><th>Time</th><th>Signal</th><th>Event</th><th>Service</th></tr></thead>
                <tbody>
                  {#each data.view.recent as row (row.event)}
                    <tr>
                      <td class="mono">{row.time}</td>
                      <td><a href={row.href}><strong>{row.signal}</strong></a></td>
                      <td>{row.event}</td>
                      <td class="mono">{row.service}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </Table>
          </Panel>
        </div>
        <div class="stack">
          <Panel title="Scope">
            {#snippet head()}<span class="mono muted">URL-backed</span>{/snippet}
            <div class="kv">
              <span class="k">service</span><span class="mono">{data.scope.service || 'all services'}</span>
              <span class="k">range</span><span class="mono">{data.scope.range}</span>
              <span class="k">inherited</span><span>carried into every signal page</span>
              <span class="k">cleared</span><span>removing a chip removes only that filter</span>
            </div>
          </Panel>
          {#if data.view.noDataNote}
            <StateBlock
              state="absent"
              title={`No ${data.view.noDataNote.signal} report in range`}
              detail={data.view.noDataNote.detail}
            />
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .tile-value {
    font: 700 24px var(--font-mono);
    margin: 0 0 6px;
  }
  .tile-unit {
    font: 11px var(--font-mono);
    color: var(--muted);
  }
</style>
