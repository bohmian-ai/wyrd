<script lang="ts">
  import '$lib/features/cards/cards.css';
  import Badge from '$lib/components/Badge.svelte';
  import Chip from '$lib/components/Chip.svelte';
  import Panel from '$lib/components/Panel.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import Table from '$lib/components/Table.svelte';
  import { filterChips } from '$lib/features/observe/core/filter-state';
  import { page } from '$app/state';
  let { data } = $props();
  const CHIP_LABELS = { kind: 'Kind', space: 'Space', status: 'Status', label: 'Label', owner: 'Owner' };
  const chips = $derived(
    filterChips(page.url.pathname, data.filters, CHIP_LABELS).filter((chip) => chip.label !== 'q')
  );
  function filterHref(key: string, value: string): string {
    const query = new URLSearchParams(page.url.search);
    if (value) query.set(key, value);
    else query.delete(key);
    return `?${query}` === '?' ? page.url.pathname : `?${query}`;
  }
</script>

<svelte:head><title>Cards · Wyrd</title></svelte:head>
<div class="cards">
  <h1>Cards</h1>
  <p class="muted">
    Everything registered in this tenant — filters are URL parameters, restorable from a pasted
    link.
  </p>
  {#if data.problem}
    <StateBlock
      state={data.problem.status === 403 ? 'unauthorized' : 'error'}
      title={data.problem.title}
      code={data.problem.code}
      detail={data.problem.remediation}
      actionLabel={data.problem.status === 403 ? undefined : 'Retry'}
      actionHref={data.problem.status === 403 ? undefined : page.url.pathname + page.url.search}
    />
  {:else if data.view}
    {#if !data.view.total}
      <StateBlock
        state="empty"
        title="No Cards registered in this tenant"
        detail="Registration is API-first: Cards enter through the SDKs, CLI or MCP."
      />
    {:else}
      <form class="filters row" method="GET" data-sveltekit-noscroll data-sveltekit-keepfocus>
        <input
          aria-label="Find a Card"
          name="q"
          value={data.filters.q}
          placeholder="⌕ Find a Card by name, kind or uid…"
        />
        {#each Object.keys(CHIP_LABELS) as key (key)}
          {#if data.filters[key]}<input type="hidden" name={key} value={data.filters[key]} />{/if}
        {/each}
        <button class="control">Search</button>
        <div class="chips" role="group" aria-label="Active filters">
          {#each chips as chip (chip.label)}
            <Chip label={chip.label} value={chip.value} removeHref={chip.removeHref} />
          {/each}
          {#if chips.length || data.filters.q}<a class="clear" href={page.url.pathname}
              >View all ⨯</a
            >{/if}
        </div>
      </form>
      <div class="inv-layout">
        <div class="inv-rail">
          <Panel title="Kind" variant="flat">
            {#snippet head()}<span class="mono muted">{data.view.kinds.length} registrable</span
              >{/snippet}
            <nav class="kind-list" aria-label="Filter by kind">
              <a href={filterHref('kind', '')} aria-current={!data.filters.kind ? 'true' : undefined}
                ><span>All kinds</span><span class="n"
                  >{data.view.kinds.reduce((sum, entry) => sum + entry.count, 0)}</span
                ></a
              >
              {#each data.view.kinds as entry (entry.kind)}
                <a
                  href={filterHref('kind', entry.kind)}
                  aria-current={data.filters.kind === entry.kind ? 'true' : undefined}
                  ><span>{entry.kind}</span><span class="n">{entry.count}</span></a
                >
              {/each}
            </nav>
          </Panel>
          {#if data.view.recent.length}
            <Panel title="Recently viewed" variant="flat">
              {#snippet head()}<span class="mono muted">this principal</span>{/snippet}
              <div class="recent-list">
                {#each data.view.recent as card (card.uid)}
                  <span
                    ><a href={`${page.url.pathname}/${card.uid}`}>{card.name}</a>
                    <span class="mono muted">{card.kind} · viewed {card.viewed}</span></span
                  >
                {/each}
              </div>
            </Panel>
          {/if}
          <p class="muted">
            Registration is API-first: Cards enter through the SDKs, CLI or MCP — this inventory is
            a read surface.
          </p>
        </div>
        <div class="inv-main">
          {#if !data.view.rows.length}
            <StateBlock
              state="empty"
              title="No matching cards"
              detail="Your search and filters are preserved."
              actionLabel="Clear search and filters"
              actionHref={page.url.pathname}
            />
          {:else}
            <Panel title="Cards" variant="raised">
              {#snippet head()}<span class="mono muted"
                  >{data.view.rows.length} of {data.view.total} match · sorted by updated</span
                >{/snippet}
              <Table label="Cards">
                <table>
                  <thead>
                    <tr>
                      <th>Name</th><th>Kind</th><th>Version</th><th>Space</th><th>Status</th>
                      <th>Owner</th><th>Updated</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each data.view.rows as row (row.uid)}
                      <tr>
                        <td data-l="name"
                          ><a href={`${page.url.pathname}/${row.uid}`}
                            ><strong>{row.name}</strong></a
                          ></td
                        >
                        <td data-l="kind" class="mono">{row.kind}</td>
                        <td data-l="version" class="mono">{row.version}</td>
                        <td data-l="space" class="mono"
                          ><a href={filterHref('space', row.space)}>{row.space}</a></td
                        >
                        <td data-l="status"
                          ><a href={filterHref('status', row.status.label)}
                            ><Badge tone={row.status.tone}>{row.status.label}</Badge></a
                          ></td
                        >
                        <td data-l="owner" class="mono"
                          ><a href={filterHref('owner', row.owner || 'unowned')}
                            >{row.owner || '— unowned'}</a
                          ></td
                        >
                        <td data-l="updated" class="mono"
                          ><time datetime={row.updatedAt}>{row.updated}</time></td
                        >
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </Table>
              <p class="muted">
                Space is a subordinate filter, never a hierarchy — every Card lives directly in the
                tenant.
              </p>
            </Panel>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>
