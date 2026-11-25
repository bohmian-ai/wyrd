<!--
  Every registered Card version, one row each in registration order, with the
  filters cards.list takes. Filters and paging are host actions, so the host
  owns the applied state (its URL) and a reload or Back restores it. A whole
  row opens its exact version.
-->
<script lang="ts">
  import type { ActionProps, ActionState, CardSummary } from '../contract.js';
  import { inventoryFilter, type CardInventoryProps } from '../cards.js';
  import { CARD_KINDS, CARD_STATUSES, type CardInventoryInput } from '../schemas.js';
  import { fmtAgo, fmtCardRef, fmtInstant, fmtShortId, parseInstant } from '../format.js';
  import ActionNote from './ActionNote.svelte';

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
  /** What the applied filters asked for, as the empty state repeats it. */
  const asked = $derived(
    [input.kind, input.status && input.status[0].toUpperCase() + input.status.slice(1), input.include_prerelease ? 'Prerelease' : undefined].filter(Boolean).join(' · ')
  );
  const expression = $derived(inventoryFilter(input.query));
  const problem = $derived(result.state === 'denied' || result.state === 'error' ? result.problem : undefined);

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

  let field = $state<HTMLInputElement>();
  /** `/` focuses the filter from anywhere outside another field. */
  function slash(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey || target?.closest('input, textarea, select, [contenteditable]')) return;
    event.preventDefault();
    field?.focus();
  }
</script>

<svelte:window onkeydown={slash} />

<section class="inventory" aria-label="Cards">
  <form
    class="find"
    aria-label="Find Cards"
    onsubmit={(event) => {
      event.preventDefault();
      filter({ query: String(new FormData(event.currentTarget).get('query') ?? '').trim() || undefined });
    }}
  >
    <label class="field">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="11" cy="11" r="7"></circle><path d="M20 20l-3.5-3.5"></path></svg>
      <span class="word">Filter</span>
      <input
        bind:this={field}
        name="query"
        aria-label="Filter Cards: plain text matches name; or a metadata query"
        value={input.query ?? ''}
        placeholder={'name, or labels.team = "support"'}
        autocomplete="off"
        spellcheck="false"
        disabled={blocked(filterState)}
      />
      <kbd>/</kbd>
    </label>
    <label class="btn status">
      <span class="word">Status</span>
      <select
        value={input.status ?? ''}
        disabled={blocked(filterState)}
        onchange={(event) => filter({ status: (event.currentTarget.value || undefined) as CardInventoryInput['status'] })}
      >
        <option value="">Any but deleted</option>
        {#each CARD_STATUSES as status (status)}<option value={status}>{status[0].toUpperCase() + status.slice(1)}</option>{/each}
      </select>
      <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M6 9l6 6 6-6"></path></svg>
    </label>
    <button
      type="button"
      class="btn"
      aria-pressed={input.include_prerelease ? 'true' : 'false'}
      disabled={blocked(filterState)}
      onclick={() => filter({ include_prerelease: !input.include_prerelease })}>Prerelease</button
    >
    <ActionNote state={filterState} />
  </form>

  <p class="caption">One row per version · registration order</p>

  <div class="body">
    <nav aria-label="Kind">
      <div class="h">Kind</div>
      <button type="button" class="kf" aria-current={input.kind === undefined ? 'true' : undefined} disabled={blocked(filterState)} onclick={() => filter({ kind: undefined })}>All kinds</button>
      {#each CARD_KINDS as kind (kind)}
        <button type="button" class="kf" aria-current={input.kind === kind ? 'true' : undefined} disabled={blocked(filterState)} onclick={() => filter({ kind })}>{kind}</button>
      {/each}
    </nav>

    <div class="main">
      {#if result.state === 'populated' && data}
        <div class="card">
          <div class="scroll">
            <div role="table" aria-label="Card versions" aria-rowcount={data.items.length + 1}>
              <div class="cr h" role="row">
                <span role="columnheader">Kind</span><span role="columnheader">Name</span><span role="columnheader">Version</span><span role="columnheader">Status</span><span role="columnheader">UID</span><span role="columnheader" class="right">Updated</span>
              </div>
              {#each data.items as item (item.card_uid)}
                {@const at = parseInstant(item.updated_at)}
                <div class="cr row" role="row">
                  <button
                    type="button"
                    class="hit"
                    aria-label="Open {fmtCardRef(item)}"
                    disabled={blocked(openState)}
                    onclick={() => open(item)}
                  ></button>
                  <span role="cell"><span class="kind">{item.kind}</span></span>
                  <span role="cell" class="name">{item.name}</span>
                  <span role="cell">{item.version}</span>
                  <span role="cell"><span class="st {item.status}">{item.status}</span></span>
                  <span role="cell" class="sub" title={item.card_uid}
                    >{fmtShortId(item.card_uid)}
                    <button type="button" class="copy" aria-label="Copy uid {item.card_uid}" onclick={() => copy(item.card_uid)}>⧉</button></span
                  >
                  <span role="cell" class="sub right"><time datetime={item.updated_at} title={fmtInstant(at)}>{fmtAgo(at)}</time></span>
                </div>
              {/each}
            </div>
          </div>
          <footer>
            <span>{data.items.length} shown</span>
            {#if copied}<span role="status">{copied}</span>{/if}
            <ActionNote state={openState} />
            {#if data.more}
              <ActionNote state={moreState} />
              <button
                type="button"
                class="btn more"
                disabled={blocked(moreState)}
                aria-busy={moreState?.state === 'pending' ? 'true' : undefined}
                onclick={() => onaction?.({ name: 'cards.more', input: { pages: (input.pages ?? 1) + 1 } })}>Load more</button
              >
            {/if}
          </footer>
        </div>
      {:else if result.state === 'empty' && data?.filtered}
        <div class="state" role="status">
          <b>No Cards match</b>
          <span>{asked}{asked && expression ? ' · ' : ''}{#if expression}<code>{expression}</code>{/if} returned nothing.</span>
          <button type="button" class="link" disabled={blocked(filterState)} onclick={() => onaction?.({ name: 'cards.filter', input: {} })}>Clear filter</button>
        </div>
      {:else if result.state === 'empty'}
        <div class="state" role="status">
          <b>No Cards registered yet</b>
          <span>Declare one from a repository:</span>
          <code>wyrd declare</code>
        </div>
      {:else if result.state === 'loading'}
        <div class="state" role="status" aria-busy="true"><b>Loading Cards…</b></div>
      {:else if problem}
        <div class="state bad" role="alert">
          <b>
            {#if result.state === 'denied'}You can't list Cards here{:else if problem.code === 'WYRD_SPEC_400_VALIDATION' || problem.code.includes('INVALID_SYNTAX')}Filter not understood{:else}{problem.title}{/if}
          </b>
          <code>{problem.code}</code>
          {#if result.state === 'denied'}<span>Ask a tenant admin for access to Cards.</span>{:else if problem.detail}<span>{problem.detail}</span>{/if}
        </div>
      {/if}
    </div>
  </div>
</section>

<style>
  .inventory {
    display: flex;
    flex-direction: column;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  .find {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .btn {
    height: 34px;
    min-width: 34px;
    box-sizing: border-box;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font: 13px var(--font-sans);
    white-space: nowrap;
    cursor: pointer;
  }
  .btn:hover:not(:disabled),
  .status:hover {
    background: var(--surface-hover);
  }
  .btn[aria-pressed='true'] {
    border-color: var(--selected-border);
    background: var(--declare-soft);
  }
  .field {
    flex: 1 1 520px;
    min-width: 0;
    height: 34px;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 10px;
    border: 1px solid var(--selected-border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--muted);
  }
  .field:focus-within {
    border-color: var(--declare);
  }
  .word {
    color: var(--muted);
    font: 12px var(--font-mono);
    text-transform: uppercase;
  }
  .status .word {
    font: 11px var(--font-sans);
    text-transform: none;
  }
  .field input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .field input::placeholder {
    color: var(--muted);
  }
  kbd {
    border: 1px solid var(--border);
    padding: 0 5px;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .status {
    position: relative;
    min-width: 190px;
    justify-content: space-between;
  }
  .status select {
    appearance: none;
    flex: 1;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--text);
    font: 13px var(--font-sans);
    text-align: right;
    cursor: pointer;
  }
  .status select option {
    background: var(--surface);
    color: var(--text);
  }
  .status:focus-within {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .caption {
    margin: 14px 0 10px;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .body {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 16px;
  }
  nav {
    flex: 0 0 180px;
    display: flex;
    flex-direction: column;
  }
  .h {
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  nav .h {
    margin: 8px 0 6px;
  }
  .kf {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin: 0 -8px;
    border: 0;
    border-radius: var(--r);
    padding: 4px 8px;
    background: none;
    color: var(--text);
    font: 12px var(--font-mono);
    text-align: left;
    cursor: pointer;
  }
  .kf:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .kf[aria-current='true'] {
    background: var(--declare-soft);
    box-shadow: inset 2px 0 0 var(--declare);
  }
  .main {
    flex: 999 1 560px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .card {
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .scroll {
    overflow-x: auto;
  }
  .cr {
    display: grid;
    grid-template-columns: 96px minmax(0, 1.4fr) 104px 112px 150px 96px;
    align-items: center;
    gap: 12px;
    min-width: 680px;
    height: 38px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-soft);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .cr.h {
    height: 30px;
    color: var(--muted);
    font-size: 11px;
  }
  .row {
    position: relative;
  }
  .row:hover,
  .row:has(.hit:focus-visible) {
    background: var(--surface-2);
  }
  /* The row's own control covers it, so a click anywhere on the row opens that version. */
  .hit {
    position: absolute;
    inset: 0;
    z-index: 0;
    border: 0;
    padding: 0;
    background: none;
    cursor: pointer;
  }
  .hit:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: -2px;
  }
  .hit:disabled {
    cursor: default;
  }
  .row > [role='cell'] {
    pointer-events: none;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    color: var(--muted);
  }
  .right {
    text-align: right;
  }
  .kind {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--selected-border);
    border-radius: var(--r);
    padding: 1px 7px;
    background: var(--declare-soft);
    color: color-mix(in srgb, var(--declare) 45%, var(--text));
    font: 10.5px var(--font-mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .st {
    display: inline-flex;
    align-items: center;
    border: 1px solid currentColor;
    border-radius: var(--r);
    padding: 0 6px;
    font: 11px var(--font-mono);
    white-space: nowrap;
  }
  .st.active {
    color: var(--ok);
  }
  .st.pending {
    color: color-mix(in srgb, var(--declare) 65%, var(--text));
  }
  .st.deprecated,
  .st.expired {
    color: var(--muted);
  }
  .st.failed {
    color: var(--danger);
  }
  .copy {
    position: relative;
    z-index: 1;
    pointer-events: auto;
    border: 0;
    padding: 0 2px;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .copy:hover {
    color: var(--declare);
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 14px;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    margin-top: -1px;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .more {
    margin-left: auto;
    height: 28px;
    font-size: 12px;
  }
  .state {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    border: 1px dashed color-mix(in srgb, var(--muted) 40%, var(--border));
    padding: 16px;
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .state.bad {
    border-color: color-mix(in srgb, var(--danger) 35%, var(--bg));
  }
  .state b {
    color: var(--text);
    font: var(--weight-strong) 13px var(--font-sans);
  }
  .state code {
    color: var(--text);
    font: 12px var(--font-mono);
  }
  .link {
    align-self: flex-start;
    border: 0;
    padding: 0;
    background: none;
    color: var(--declare);
    font: 12px var(--font-mono);
    cursor: pointer;
  }
  .link:hover:not(:disabled) {
    text-decoration: underline;
  }
  :is(button, select, input):focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .field input:focus-visible,
  .status select:focus-visible {
    outline: 0;
  }
  :is(button, select, input):disabled {
    cursor: default;
  }
</style>
