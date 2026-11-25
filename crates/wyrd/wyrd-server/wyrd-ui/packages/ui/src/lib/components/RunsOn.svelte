<!--
  Where an exact Verifier version runs: each Service or Agent that binds it,
  the subject of each binding and when it runs. A refused owner read keeps
  its row.
-->
<script lang="ts">
  import type { ActionProps, CardRef } from '../contract.js';
  import type { RunsOnProps } from '../verifier.js';
  import { fmtCardRef } from '../format.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result, actions = {}, onaction }: RunsOnProps & ActionProps = $props();

  const state = $derived(actions['card.open']);
  const disabled = $derived(!onaction || state?.state === 'pending' || state?.state === 'unavailable');
  const open = (ref: CardRef) => onaction?.({ name: 'card.open', input: { card_ref: ref } });
</script>

<Panel title="Runs on" note="Bindings that pin this exact version">
  {#if result.state === 'populated'}
    <ul class="flush">
      {#each result.data as owner (fmtCardRef(owner.ref))}
        <li>
          <span class="owner">
            <span class="kind">{owner.ref.kind}</span>
            <button type="button" class="open" {disabled} title={fmtCardRef(owner.ref)} onclick={() => open(owner.ref)}>{owner.ref.name}@{owner.ref.version}</button>
          </span>
          {#if owner.bindings.state === 'populated'}
            {#each owner.bindings.data as binding, index (index)}
              <span class="muted">{binding.subject ? `component ${binding.subject}` : 'the whole Card'} · {binding.when}</span>
            {:else}
              <span class="muted">references this version without a binding</span>
            {/each}
          {:else}
            <span class="muted">{owner.bindings.problem.title} ({owner.bindings.problem.code})</span>
          {/if}
        </li>
      {/each}
    </ul>
    <ActionNote {state} />
  {:else if result.state === 'empty'}
    <EmptyState state="empty" title={result.reason} />
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading where {input.cardRef.name}@{input.cardRef.version} runs" />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</Panel>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    font: 12px var(--font-mono);
  }
  li {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 9px 16px;
    border-bottom: 1px solid var(--border-soft);
    overflow-wrap: anywhere;
  }
  li:last-child {
    border-bottom: 0;
  }
  .owner {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .kind {
    border: 1px solid var(--selected-border);
    border-radius: var(--r);
    padding: 0 6px;
    background: var(--declare-soft);
    font: 10px/16px var(--font-mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
</style>
