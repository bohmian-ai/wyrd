<!--
  One exact Card version: name, kind, version select, lifecycle status,
  copyable identity, host tabs and Open in Observe. The version listing has
  its own state, so a failed listing never hides the Card itself.
-->
<script lang="ts">
  import type { ActionProps, CardRef } from '../contract.js';
  import type { CardHeaderProps } from '../loaders.js';
  import { fmtCardRef } from '../format.js';
  import ActionButton from './ActionButton.svelte';
  import EmptyState from './EmptyState.svelte';
  import StatusPill, { type Tone } from './StatusPill.svelte';

  let { input, result, actions = {}, onaction }: CardHeaderProps & ActionProps = $props();

  const TONES: Record<string, Tone> = {
    active: 'ok',
    pending: 'running',
    deprecated: 'warn',
    expired: 'warn',
    failed: 'danger',
    deleted: 'neutral'
  };

  const card = $derived(result.state === 'populated' ? result.data.card : undefined);
  const versions = $derived(result.state === 'populated' ? result.data.versions : undefined);
  const ref = $derived<CardRef | undefined>(
    card && { kind: card.kind, name: card.metadata.name, version: input.cardRef.version, space: card.metadata.space ?? input.cardRef.space }
  );
  const current = $derived(
    versions?.state === 'populated' ? versions.data.items.find((item) => item.version === input.cardRef.version) : undefined
  );
  const unavailable = { state: 'unavailable', reason: 'This host offers no navigation' } as const;

  /** Local clipboard feedback; copying is not a host action. */
  let copied = $state<'Copied' | 'Copy failed' | undefined>();
  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = 'Copied';
    } catch {
      copied = 'Copy failed';
    }
  }
</script>

{#if result.state === 'populated' && card && ref}
  <header class="card-header">
    <div class="title">
      <h2>{card.metadata.name}</h2>
      <StatusPill label={card.kind} />
      {#if current}<StatusPill tone={TONES[current.status]} label={current.status} />{/if}
      <label class="version">
        <span>Version</span>
        <select
          disabled={!onaction || versions?.state !== 'populated'}
          value={input.cardRef.version}
          onchange={(event) => onaction?.({ name: 'card.version', input: { card_ref: { ...ref, version: event.currentTarget.value } } })}
        >
          {#if versions?.state === 'populated'}
            {#each versions.data.items as item (item.version)}<option value={item.version}>{item.version}</option>{/each}
            {#if !versions.data.items.some((item) => item.version === input.cardRef.version)}
              <option value={input.cardRef.version}>{input.cardRef.version}</option>
            {/if}
          {:else}
            <option value={input.cardRef.version}>{input.cardRef.version}</option>
          {/if}
        </select>
      </label>
      {#if versions?.state === 'populated' && versions.data.truncated}
        <span class="note">Showing the newest {versions.data.items.length} versions</span>
      {:else if versions?.state === 'denied' || versions?.state === 'error'}
        <span class="note" role="note">Versions unavailable: {versions.problem.title} ({versions.problem.code})</span>
      {/if}
    </div>

    <div class="identity">
      <code>{fmtCardRef(ref)}</code>
      <button type="button" onclick={() => copy(fmtCardRef(ref))}>Copy ref</button>
      {#if card.metadata.uid}
        <code title="Card uid">{card.metadata.uid}</code>
        <button type="button" onclick={() => copy(card.metadata.uid ?? '')}>Copy uid</button>
      {/if}
      {#if copied}<span class="note" role="status">{copied}</span>{/if}
      <span class="primary">
        <ActionButton
          primary
          label="Open in Observe"
          state={onaction ? actions['card.open_in_observe'] : unavailable}
          onclick={() => onaction?.({ name: 'card.open_in_observe', input: { card_ref: ref } })}
        />
      </span>
    </div>

    {#if input.tabs?.length}
      <nav aria-label="{card.metadata.name} sections">
        {#each input.tabs as tab (tab.id)}
          <button
            type="button"
            aria-current={tab.id === (input.tab ?? input.tabs[0].id) ? 'page' : undefined}
            disabled={!onaction}
            onclick={() => onaction?.({ name: 'card.tab', input: { tab: tab.id } })}>{tab.label}</button
          >
        {/each}
      </nav>
    {/if}
  </header>
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {input.cardRef.name}@{input.cardRef.version}" />
{:else if result.state === 'denied' || result.state === 'error'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  .card-header {
    display: grid;
    gap: 10px;
    min-width: 0;
    border-bottom: 1px solid var(--border);
    padding-bottom: 10px;
    color: var(--text);
    font-family: var(--font-sans);
  }
  .title,
  .identity {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font: var(--weight-strong) 20px/1.2 var(--font-sans);
    overflow-wrap: anywhere;
  }
  .version {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 12px;
  }
  select,
  button {
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 3px 8px;
    background: var(--surface);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  button {
    cursor: pointer;
    font-family: var(--font-sans);
  }
  button:hover:not(:disabled) {
    background: var(--surface-hover);
  }
  :is(select, button):focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  code {
    min-width: 0;
    overflow-wrap: anywhere;
    font: 12px var(--font-mono);
  }
  .note {
    color: var(--muted);
    font-size: 12px;
  }
  .primary {
    margin-left: auto;
  }
  nav {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  nav button[aria-current='page'] {
    border-color: var(--declare);
    background: var(--declare-soft);
  }
</style>
