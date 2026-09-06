<script lang="ts">
  import '$lib/features/observe/observe.css';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import ObserveNav from '$lib/features/observe/ObserveNav.svelte';
  import ScopeChips from '$lib/features/observe/ScopeChips.svelte';
  import { filterChips, readScope } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const base = $derived(page.url.pathname.replace(/\/observe\/dashboards$/, ''));
  const scope = $derived(readScope(page.url.searchParams));
  const chips = $derived(
    filterChips(page.url.pathname, { folder: data.filters.folder, tag: data.filters.tag, q: data.filters.q })
  );
  function selectHref(id: string): string {
    const query = new URLSearchParams(page.url.search);
    query.set('selected', id);
    return `?${query}`;
  }
  function filterHref(key: string, value: string): string {
    const query = new URLSearchParams(page.url.search);
    query.set(key, value);
    return `?${query}`;
  }
</script>

<svelte:head><title>Dashboards · Observe · Wyrd</title></svelte:head>
<div class="observe">
  <h1>Dashboards</h1>
  <p class="muted">Read-only dashboards published for this tenant.</p>
  <ObserveNav base={`${base}/observe`} current="Dashboards" {scope} />
  <div class="stack">
    <form class="filters row" method="GET">
      <input aria-label="Search dashboards" name="q" value={data.filters.q} placeholder="Search dashboards…" />
      <button class="control">Search</button>
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
        {#if !data.view.rows.length}
          <StateBlock
            state="empty"
            title="No matching dashboards"
            detail="Your search and filters are preserved."
            actionLabel="Clear search and filters"
            actionHref={page.url.pathname}
          />
        {:else}
          <Panel title="Dashboards" variant="raised">
            {#snippet head()}<span class="mono muted">{data.view.rows.length}</span>{/snippet}
            <Table label="Dashboards">
              <table>
                <thead><tr><th>Name</th><th>Folder</th><th>Tags</th><th>Owner</th><th>Updated</th></tr></thead>
                <tbody>
                  {#each data.view.rows as row (row.id)}
                    <tr class={data.view.selected?.id === row.id ? 'sel' : ''}>
                      <td><a href={selectHref(row.id)}><strong>{row.name}</strong></a></td>
                      <td class="mono"><a href={filterHref('folder', row.folder)}>{row.folder}</a></td>
                      <td class="mono">
                        {#each row.tags as tag, i (tag)}{#if i > 0},&nbsp;{/if}<a href={filterHref('tag', tag)}>{tag}</a>{/each}
                      </td>
                      <td class="mono">{row.owner}</td>
                      <td class="mono">{row.updated}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </Table>
            <p class="muted">Open → the dashboard detail. No create, edit, alert or schedule action exists on this surface.</p>
          </Panel>
        {/if}
        <div class="stack">
          {#if data.view.selected}
            {@const selected = data.view.selected}
            <Panel title="Preview" variant="raised">
              {#snippet head()}<span class="mono muted">{selected.name}</span>{/snippet}
              <div class="kv">
                <span class="k">panels</span><span class="mono">{data.view.selected.panels} · {data.view.selected.defaultRange} default range</span>
                <span class="k">variables</span><span class="mono">{data.view.selected.variables} · read-only</span>
                <span class="k">owner</span><span class="mono">{data.view.selected.owner}</span>
                <span class="k">folder</span><span class="mono">{data.view.selected.folder}</span>
                <span class="k">updated</span><span class="mono">{data.view.selected.updated}</span>
              </div>
              <p>
                <a class="control primary" href={`${page.url.pathname}/${data.view.selected.id}`}>Open dashboard</a>
              </p>
            </Panel>
            <div class="notice">
              <p class="mono muted">NOT OFFERED HERE</p>
              <p class="muted">
                No Create, Edit, Save, Alert rule, Schedule or subscription. Dashboards are read-only —
                authoring is out of scope, not hidden behind a permission.
              </p>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</div>
