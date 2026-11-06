<!--
  Every registered Card version, one row each in registration order, with the
  filters cards.list takes. Filters and paging are host actions, so the host
  owns the applied state (its URL) and a reload or Back restores it.
-->
<script lang="ts">
  import type { ActionProps, ActionState, CardSummary } from '../contract.js';
  import type { CardInventoryProps } from '../cards.js';
  import { CARD_KINDS, CARD_STATUSES, type CardInventoryInput } from '../schemas.js';
  import { fmtCardRef, fmtInstant, parseInstant } from '../format.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import StatusPill, { CARD_TONES } from './StatusPill.svelte';

  let { input, result, actions = {}, onaction }: CardInventoryProps & ActionProps = $props();

  const filterState = $derived(actions['cards.filter']);
  const moreState = $derived(actions['cards.more']);
  const openState = $derived(actions['card.open']);
  const blocked = (state: ActionState | undefined) => !onaction || state?.state === 'pending' || state?.state === 'unavailable';
  const filters = $derived({
    query: input.query,
    kind: input.kind,
    status: input.status,
    include_prerelease: input.include_prerelease
  });
  const data = $derived(result.state === 'populated' || result.state === 'empty' ? result.data : undefined);

  /** Apply `change` over the current filters; unset values leave the request. */
  function filter(change: Partial<Omit<CardInventoryInput, 'pages'>>) {
    const next = Object.fromEntries(Object.entries({ ...filters, ...change }).filter(([, value]) => value !== undefined && value !== '' && value !== false));
    onaction?.({ name: 'cards.filter', input: next });
  }

  function open(item: CardSummary) {
    onaction?.({ name: 'card.open', input: { card_ref: { kind: item.kind, name: item.name, version: item.version, uid: item.card_uid } } });
  }

  let copied = $state<string | undefined>();
  async function copy(uid: string) {
    try {
      await navigator.clipboard.writeText(uid);
      copied = `Copied ${uid}`;
    } catch {
      copied = 'Copy failed';
    }
  }
</script>

<section class="inventory" aria-label="Cards">
  <form
    class="filters"
    onsubmit={(event) => {
      event.preventDefault();
      filter({ query: String(new FormData(event.currentTarget).get('query') ?? '').trim() || undefined });
    }}
  >
    <label class="query">
      <span>Filter</span>
      <input
        name="query"
        value={input.query ?? ''}
        placeholder={'name, or labels.team = "search"'}
        autocomplete="off"
        spellcheck="false"
        disabled={blocked(filterState)}
      />
    </label>
    <label>
      <span>Status</span>
      <select
        value={input.status ?? ''}
        disabled={blocked(filterState)}
        onchange={(event) => filter({ status: (event.currentTarget.value || undefined) as CardInventoryInput['status'] })}
      >
        <option value="">Any but deleted</option>
        {#each CARD_STATUSES as status (status)}<option value={status}>{status}</option>{/each}
      </select>
    </label>
    <label class="check">
      <input
        type="checkbox"
        checked={input.include_prerelease ?? false}
        disabled={blocked(filterState)}
        onchange={(event) => filter({ include_prerelease: event.currentTarget.checked })}
      />
      <span>Prerelease</span>
    </label>
    <ActionNote state={filterState} />
  </form>

  <div class="body">
    <nav aria-label="Kinds">
      <button type="button" aria-current={input.kind === undefined ? 'page' : undefined} disabled={blocked(filterState)} onclick={() => filter({ kind: undefined })}>All kinds</button>
      {#each CARD_KINDS as kind (kind)}
        <button type="button" aria-current={input.kind === kind ? 'page' : undefined} disabled={blocked(filterState)} onclick={() => filter({ kind })}>{kind}</button>
      {/each}
    </nav>

    <div class="rows">
      {#if result.state === 'populated' && data}
        <p class="caption">One row per version · registration order</p>
        <table>
          <thead><tr><th>Kind</th><th>Name</th><th>Version</th><th>Status</th><th>UID</th><th>Updated</th></tr></thead>
          <tbody>
            {#each data.items as item (item.card_uid)}
              <tr>
                <td><span class="kind">{item.kind}</span></td>
                <td>
                  <button type="button" class="open" title={fmtCardRef(item)} disabled={blocked(openState)} onclick={() => open(item)}>{item.name}</button>
                </td>
                <td>{item.version}</td>
                <td><StatusPill tone={CARD_TONES[item.status]} label={item.status} /></td>
                <td>
                  <span title={item.card_uid}>{item.card_uid.slice(0, 8)}</span>
                  <button type="button" class="icon" aria-label="Copy uid {item.card_uid}" onclick={() => copy(item.card_uid)}>⧉</button>
                </td>
                <td><time datetime={item.updated_at}>{fmtInstant(parseInstant(item.updated_at))}</time></td>
              </tr>
            {/each}
          </tbody>
        </table>
        <footer>
          <span>{data.items.length} shown</span>
          {#if copied}<span role="status">{copied}</span>{/if}
          <ActionNote state={openState} />
          {#if data.more}
            <button type="button" class="more" disabled={blocked(moreState)} aria-busy={moreState?.state === 'pending' ? 'true' : undefined} onclick={() => onaction?.({ name: 'cards.more', input: { pages: (input.pages ?? 1) + 1 } })}>Load more</button>
            <ActionNote state={moreState} />
          {/if}
        </footer>
      {:else if result.state === 'empty' && data?.filtered}
        <EmptyState state="empty" title={result.reason} detail="Clear the filter or widen the status." />
        <button type="button" class="more" disabled={blocked(filterState)} onclick={() => onaction?.({ name: 'cards.filter', input: {} })}>Clear filter</button>
      {:else if result.state === 'empty'}
        <EmptyState state="empty" title={result.reason} detail="Register one with wyrd declare <path>." />
      {:else if result.state === 'loading'}
        <EmptyState state="loading" title="Loading Cards" />
      {:else if result.state === 'denied' || result.state === 'error'}
        <EmptyState state={result.state} problem={result.problem} />
      {/if}
    </div>
  </div>
</section>

<style>
  .inventory {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .query {
    flex: 1 1 280px;
  }
  .check {
    flex-direction: row;
    align-items: center;
    height: 32px;
    gap: 6px;
  }
  input:not([type='checkbox']),
  select {
    height: 32px;
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 10px;
    background: var(--surface-2);
    color: var(--text);
    font: 12px var(--font-mono);
    letter-spacing: 0;
    text-transform: none;
  }
  .body {
    display: grid;
    grid-template-columns: minmax(120px, 160px) minmax(0, 1fr);
    gap: 16px;
  }
  @media (max-width: 640px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
    }
    nav {
      flex-direction: row !important;
      flex-wrap: wrap;
    }
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  nav button {
    border: 0;
    border-left: 2px solid transparent;
    padding: 5px 10px;
    background: none;
    color: var(--muted);
    font: 13px var(--font-sans);
    text-align: left;
    cursor: pointer;
  }
  nav button:hover:enabled {
    color: var(--text);
  }
  nav button[aria-current='page'] {
    border-left-color: var(--declare);
    color: var(--text);
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    overflow-x: auto;
  }
  .caption {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-mono);
  }
  th {
    height: 28px;
    padding: 0 12px 0 0;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-align: left;
    text-transform: uppercase;
  }
  td {
    height: 36px;
    padding: 0 12px 0 0;
    border-bottom: 1px solid var(--border-soft);
    white-space: nowrap;
  }
  .kind {
    border: 1px solid color-mix(in srgb, var(--declare) 45%, var(--border));
    border-radius: var(--r);
    padding: 1px 6px;
    background: var(--declare-soft);
    color: var(--declare);
    font-size: 11px;
  }
  .open,
  .icon {
    border: 0;
    padding: 0;
    background: none;
    font: inherit;
    cursor: pointer;
  }
  .open {
    color: var(--declare);
  }
  .open:hover:enabled {
    text-decoration: underline;
  }
  .icon {
    margin-left: 4px;
    color: var(--muted);
  }
  .icon:hover {
    color: var(--declare);
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .more {
    align-self: flex-start;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 12px;
    background: var(--surface-2);
    color: var(--text);
    font: 13px var(--font-sans);
    cursor: pointer;
  }
  .more:hover:enabled {
    background: var(--surface-hover);
  }
  :is(button, input, select):focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  :is(button, input, select):disabled {
    cursor: default;
  }
</style>
