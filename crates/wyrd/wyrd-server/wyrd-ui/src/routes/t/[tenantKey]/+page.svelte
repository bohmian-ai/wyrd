<script lang="ts">
  import type { PageProps } from './$types';
  import Panel from '$lib/components/Panel.svelte';
  import Badge from '$lib/components/Badge.svelte';
  import StateBlock from '$lib/components/StateBlock.svelte';
  import { fmtCount } from '$lib/format';
  let { data }: PageProps = $props();
  let base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
</script>

<svelte:head><title>Home · {data.tenant.name} · Wyrd</title></svelte:head>
<div class="heading">
  <div><h1>Home</h1><p>What needs you, what changed, and where to pick work back up.</p></div>
  <div class="actions">
    <form role="search" method="GET" action={`${base}/cards`}>
      <input aria-label="Find a Card" name="q" type="search" placeholder="⌕ Find a Card by name, kind or uid…" />
    </form>
    <a class="app-control primary" href={`${base}/changes/new`}>+ New Change Request</a>
  </div>
</div>

{#if data.problem}
  <StateBlock state="error" title="Home is unavailable" detail={data.serverUnavailable ? "Mock data is disabled. The Wyrd server connection is not implemented yet." : data.problem.remediation} code={data.problem.code} actionHref={base} actionLabel="Retry" />
{:else if data.home}
  <div class="home-grid">
    <div class="work">
      <div class="attention">
        <Panel variant="accent">
          {#snippet head()}<h2>Requires your action</h2><span>{data.home.attention.length} items</span>{/snippet}
          {#if data.home.attention.length}
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <div class="scroller" role="region" aria-label="Requires your action" tabindex="0">
              <table class="attention-table"><tbody>
                {#each data.home.attention as item (item.href)}
                  <tr><td><a href={item.href}>{item.title}</a></td><td class="muted">{item.detail}</td><td class="age">{item.age}</td><td><Badge tone={item.tone}>{item.status}</Badge></td></tr>
                {/each}
              </tbody></table>
            </div>
          {:else}
            <StateBlock state="empty" title="Nothing needs you right now." detail="Items that need your attention will appear here." />
          {/if}
        </Panel>
      </div>
      <Panel>
        {#snippet head()}<h2>Recent changes</h2><a class="context-link" href={`${base}/changes`}>View all in Changes →</a>{/snippet}
        {#if data.home.changes.length}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <div class="scroller" role="region" aria-label="Recent changes" tabindex="0">
            <table class="changes-table">
              <thead><tr><th>Change</th><th>Owner</th><th>Lifecycle</th><th>Claims</th><th>Age</th></tr></thead>
              <tbody>{#each data.home.changes as item (item.href)}
                <tr><td><a href={item.href}>{item.id} {item.title}</a></td><td class="muted">{item.owner}</td><td><Badge tone={item.tone}>{item.status}</Badge></td><td>{item.claims.satisfied}/{item.claims.total}</td><td class="age">{item.age}</td></tr>
              {/each}</tbody>
            </table>
          </div>
        {:else}<p class="empty">No recent changes in this tenant.</p>{/if}
      </Panel>
      <Panel>
        {#snippet head()}<h2>Recently viewed Cards</h2><span>last 7 days</span>{/snippet}
        {#if data.home.cards.length}
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <div class="scroller" role="region" aria-label="Recently viewed Cards" tabindex="0">
            <table class="cards-table">
              <thead><tr><th>Name</th><th>Kind</th><th>Version</th><th>Space</th><th>Status</th><th>Seen</th></tr></thead>
              <tbody>{#each data.home.cards as item (item.href)}
                <tr><td><a href={item.href}>{item.name}</a></td><td class="muted">{item.kind}</td><td>{item.version}</td><td class="muted">{item.space}</td><td><Badge tone={item.tone}>{item.status}</Badge></td><td class="age">{item.seen}</td></tr>
              {/each}</tbody>
            </table>
          </div>
        {:else}<p class="empty">No recently viewed Cards in this tenant.</p>{/if}
      </Panel>
    </div>
    <aside aria-label="Workspace summaries and recent work">
      {#each data.home.summaries as item (item.href)}
        <Panel variant="flat">
          {#snippet head()}<h2>{item.label}</h2>{/snippet}
          <div class="summary" class:lime={item.label === 'Observe'}>
            <strong>{item.signal ? '! ' : ''}{fmtCount(item.value)} {item.unit}</strong>
            <p>{item.detail}</p>
            <a href={item.href}>Open {item.label} →</a>
          </div>
        </Panel>
      {/each}
      <Panel variant="flat">
        {#snippet head()}<h2>Pick work back up</h2><span>recent</span>{/snippet}
        <div class="recent">
          {#each data.home.recent as item (item.href)}
            <div><a href={item.href}>{item.title}</a><p>{item.detail}</p></div>
          {:else}<p>No recent work in this tenant.</p>{/each}
        </div>
      </Panel>
    </aside>
  </div>
{/if}

<style>
  .heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    flex-wrap: wrap;
    min-height: 100px;
    padding: 24px 24px 14px;
    margin: -24px -24px 24px;
    border-bottom: 2px solid var(--border);
  }
  h1 {
    font: 700 24px var(--font-display);
  }
  .heading p {
    font: 12px var(--font-sans);
    color: var(--muted);
    margin-top: 4px;
  }
  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
  }
  .actions input {
    width: 284px;
    max-width: 100%;
    min-width: 0;
    padding: 7px 12px;
    background: var(--surface-2);
    color: var(--text);
    border: 2px solid transparent;
    border-radius: var(--r);
    font: 11px var(--font-mono);
  }
  .actions input:focus {
    border-color: var(--brand-strong);
  }
  .actions a {
    text-decoration: none;
    padding: 7px 24px;
    font-size: 11px;
  }
  .home-grid {
    display: grid;
    grid-template-columns: minmax(0, 2.08fr) minmax(0, 1fr);
    gap: 24px;
    align-items: start;
  }
  .work {
    display: grid;
    gap: 24px;
    min-width: 0;
  }
  aside {
    display: grid;
    gap: 14px;
    min-width: 0;
  }
  h2 {
    font: inherit;
  }
  .home-grid :global(.wy-panel-head > span),
  .context-link {
    font: 400 11px var(--font-mono);
    color: var(--muted);
    text-decoration: none;
  }
  .attention :global(.wy-panel) {
    min-height: 214px;
  }
  /* Observe is the client/runtime plane — its numeral carries the lime voice, with the
     "!" glyph and label as the textual channel. */
  .summary.lime strong {
    color: var(--lime-text);
  }
  .work :global(.wy-panel-body) {
    padding: 0 10px 10px;
  }
  .scroller {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-mono);
    white-space: nowrap;
  }
  th {
    text-align: left;
    font-size: 9px;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
    border-bottom: 2px solid var(--border);
    height: 32px;
  }
  th,
  td {
    padding: 6px 12px;
  }
  td {
    height: 38px;
    border-bottom: 2px solid var(--surface-2);
  }
  tbody tr {
    transition: background 120ms ease;
  }
  tbody tr:hover {
    background: var(--brand-soft);
  }
  @media (prefers-reduced-motion: reduce) {
    tbody tr {
      transition: none;
    }
  }
  tr:last-child td {
    border-bottom: 0;
  }
  .attention-table td {
    height: 42px;
    padding-left: 12px;
    padding-right: 8px;
  }
  /* Every row leads with its human-readable title; identifiers live in the muted column. */
  .attention-table a,
  .changes-table a {
    font-weight: 700;
  }
  table {
    table-layout: fixed;
    min-width: 740px;
  }
  .attention-table td:first-child {
    width: 50%;
  }
  .attention-table td:nth-child(2) {
    width: 30%;
  }
  .attention-table td:nth-child(3) {
    width: 6%;
  }
  .attention-table td:last-child {
    width: 14%;
  }
  .changes-table th:first-child {
    width: 55%;
  }
  .changes-table th:nth-child(2) {
    width: 15%;
  }
  .changes-table th:nth-child(3) {
    width: 16%;
  }
  .changes-table th:nth-child(4) {
    width: 8%;
  }
  .changes-table th:last-child {
    width: 6%;
  }
  .cards-table th:first-child {
    width: 30%;
  }
  .cards-table th:nth-child(2) {
    width: 15%;
  }
  .cards-table th:nth-child(3) {
    width: 12%;
  }
  .cards-table th:nth-child(4) {
    width: 13%;
  }
  .cards-table th:nth-child(5) {
    width: 22%;
  }
  .cards-table th:last-child {
    width: 8%;
  }
  td a {
    color: var(--text);
    text-decoration: none;
  }
  a:hover {
    text-decoration: underline;
  }
  tbody tr:has(a):hover {
    background: var(--surface-2);
  }
  .muted,
  .age {
    color: var(--muted);
  }
  .age {
    text-align: right;
  }
  .cards-table a,
  .recent a,
  .summary a {
    color: var(--brand-strong);
    text-decoration: none;
    font-weight: 700;
  }
  .summary {
    padding: 0 4px;
    line-height: 14px;
  }
  .summary strong {
    display: block;
    font: 700 22px/26px var(--font-display);
    margin-bottom: 4px;
  }
  .summary p,
  .summary a {
    font-family: var(--font-mono);
    font-size: 10px;
  }
  .summary a {
    display: block;
  }
  .summary p {
    color: var(--muted);
    margin-bottom: 3px;
  }
  .recent {
    display: grid;
    gap: 8px;
    padding: 0 4px;
    font: 12px var(--font-mono);
  }
  .recent p {
    color: var(--muted);
    font-size: 10px;
    margin-top: 2px;
  }
  .empty {
    padding: 16px 2px;
    color: var(--muted);
    font-size: 12px;
  }
  @media (max-width: 1100px) {
    .home-grid {
      grid-template-columns: minmax(0, 1fr);
    }
    aside {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 767px) {
    .heading {
      margin: -22px -18px 22px;
      padding: 22px 18px;
    }
    .actions,
    .actions form,
    .actions input {
      width: 100%;
    }
    aside {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
