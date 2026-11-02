<!--
  Value counts of each facet column under the list's own WHERE and range.
  Choosing a value asks the host to filter by it; a null value reads "not set"
  and filters by IS NULL. Values already in the WHERE render pressed. A facet
  shows its first values and expands to every value read; when the read found
  more than it returns, the count of hidden values says so with "+".
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import { FACET_VALUES, STATUS_NAMES, columnLabel, type FacetRailProps } from '../loaders.js';
  import { fmtCount } from '../format.js';
  import { numeric } from '../plot.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';

  let { input, result, actions = {}, onaction }: FacetRailProps & ActionProps = $props();

  const filterState = $derived(actions['facet.filter']);
  const blocked = $derived(!onaction || filterState?.state === 'pending' || filterState?.state === 'unavailable');
  const active = (column: string, value: unknown) =>
    (input.where ?? []).some((filter) => filter.column === column && filter.value === value && !filter.not);
  /** Values a collapsed facet shows. */
  const FIRST = 5;
  /** Facets the reader expanded. */
  let expanded = $state.raw<ReadonlySet<string>>(new Set());
</script>

{#if result.state === 'populated' || result.state === 'empty'}
  <nav class="rail" aria-label="Filter {input.signal}">
    {#if onaction}<p class="hint">Click a value to add it to WHERE</p>{/if}
    {#each result.data.facets as { column, values } (column)}
      {@const read = values.rows.slice(0, FACET_VALUES)}
      {@const more = values.rows.length > FACET_VALUES}
      {@const open = expanded.has(column)}
      <section>
        <h3 title={column}>{columnLabel(column)}</h3>
        {#if values.rows.length === 0}
          <p class="none">No {column} recorded in this range.</p>
        {:else}
          <ul>
            {#each open ? read : read.slice(0, FIRST) as [value, n], index (index)}
              <li>
                <button
                  type="button"
                  aria-pressed={active(column, value)}
                  disabled={blocked}
                  onclick={() =>
                    onaction?.({ name: 'facet.filter', input: { signal: input.signal, column, value: value as string | number | boolean | null } })}
                >
                  <span
                    class="value"
                    class:unset={value === null}
                    class:error={column === 'status_code' && value !== null && numeric(value) === 2}
                    >{value === null ? 'not set' : column === 'status_code' ? (STATUS_NAMES[Number(value)] ?? String(value)) : String(value)}</span
                  >
                  <span class="count">{fmtCount(numeric(n))}</span>
                </button>
              </li>
            {/each}
          </ul>
          {#if !open && (read.length > FIRST || more)}
            <button type="button" class="more" aria-expanded="false" onclick={() => (expanded = new Set([...expanded, column]))}
              >+ {read.length - FIRST}{more ? '+' : ''} more</button
            >
          {:else if open && more}
            <p class="none">More values exist; filter to narrow them.</p>
          {/if}
        {/if}
      </section>
    {/each}
    <ActionNote state={filterState} />
  </nav>
  {#if result.state === 'empty'}<p class="none">{result.reason}</p>{/if}
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {input.facets.join(', ')}" />
{:else}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .rail {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
    color: var(--text);
    font-family: var(--font-sans);
  }
  h3 {
    margin: 0 0 6px;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .hint {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-sans);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  button {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    border: 0;
    border-radius: var(--r);
    padding: 4px 0;
    background: none;
    color: inherit;
    font: 12px var(--font-mono);
    text-align: left;
    cursor: pointer;
  }
  button:enabled:hover {
    background: var(--surface-hover);
  }
  button[aria-pressed='true'] {
    color: var(--declare);
  }
  button[aria-pressed='true'] .value::before {
    content: '✓ ';
  }
  button:disabled {
    cursor: default;
  }
  button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 1px;
  }
  .value {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more {
    width: auto;
    color: var(--declare);
    font-size: 11px;
  }
  .error {
    color: var(--danger);
  }
  .unset {
    padding-right: 2px;
    color: var(--muted);
  }
  .count {
    color: var(--muted);
    font: 12px var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  .none {
    margin: 0;
    color: var(--muted);
    font: 12px var(--font-sans);
  }
</style>
