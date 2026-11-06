<!--
  An exact Card's server-derived relationships: the Cards it references, then
  the Cards that reference it, each an exact version the host can open.
-->
<script lang="ts">
  import type { ActionProps, CardRef } from '../contract.js';
  import type { RelationshipListProps } from '../cards.js';
  import { fmtCardRef } from '../format.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result, actions = {}, onaction }: RelationshipListProps & ActionProps = $props();

  const state = $derived(actions['card.open']);
  const disabled = $derived(!onaction || state?.state === 'pending' || state?.state === 'unavailable');
  const groups = $derived(
    result.state === 'populated'
      ? ([
          ['references', result.data.outbound],
          ['used by', result.data.inbound]
        ] as const)
      : []
  );
  const open = (ref: CardRef) => onaction?.({ name: 'card.open', input: { card_ref: ref } });
</script>

<Panel title="Relationships" note="server-derived">
  {#if result.state === 'populated'}
    <ul class="flush">
      {#each groups as [edge, refs] (edge)}
        {#each refs as relationship (fmtCardRef(relationship.ref))}
          <li>
            <button type="button" {disabled} aria-label="{relationship.alias ?? edge} {fmtCardRef(relationship.ref)}" onclick={() => open(relationship.ref)}>
              <span class="edge">{relationship.alias ?? edge}</span>
              <span class="kind">{relationship.ref.kind}</span>
              <span class="ref">{relationship.ref.name}@{relationship.ref.version}</span>
            </button>
          </li>
        {/each}
      {/each}
    </ul>
    <ActionNote {state} />
  {:else if result.state === 'empty'}
    <EmptyState state="empty" title={result.reason} />
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading relationships of {input.cardRef.name}" />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</Panel>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li + li {
    border-top: 1px solid var(--border-soft);
  }
  button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    border: 0;
    padding: 8px 16px;
    background: none;
    color: var(--text);
    font: 12px var(--font-mono);
    text-align: left;
  }
  button:enabled {
    cursor: pointer;
  }
  button:enabled:hover {
    background: var(--surface-2);
  }
  button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: -2px;
  }
  .edge {
    flex: 0 0 96px;
    color: var(--muted);
    font-size: 10.5px;
    white-space: nowrap;
  }
  .kind {
    border: 1px solid var(--selected-border);
    border-radius: var(--r);
    padding: 1px 6px;
    background: var(--declare-soft);
    color: color-mix(in srgb, var(--declare) 45%, var(--text));
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .ref {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
