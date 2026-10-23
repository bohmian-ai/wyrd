<!--
  One exact Card version: name, kind, version select, lifecycle status,
  copyable identity, host tabs and Open in Observe. The version listing has
  its own state, so a failed listing never hides the Card itself.
-->
<script lang="ts">
  import type { ActionProps, ActionState, CardRef } from '../contract.js';
  import type { CardHeaderProps } from '../loaders.js';
  import { fmtCardRef, fmtInstant, parseInstant } from '../format.js';
  import ActionButton from './ActionButton.svelte';
  import ActionNote from './ActionNote.svelte';
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
    card && { kind: card.kind, name: card.metadata.name, version: input.cardRef.version }
  );
  const items = $derived(versions?.state === 'populated' ? versions.data.items : []);
  const current = $derived(items.find((item) => item.version === input.cardRef.version));
  /** The newest listed version; the select marks it `current`. */
  const newest = $derived(items[0]?.version);
  const unavailable = { state: 'unavailable', reason: 'This host offers no navigation' } as const;
  const versionState = $derived(actions['card.version']);
  const tabState = $derived(actions['card.tab']);
  /** A pending or unavailable action takes no further input, so an intent is never sent twice. */
  const blocked = (state: ActionState | undefined) => state?.state === 'pending' || state?.state === 'unavailable';

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
    <div class="top">
      <div class="identity">
        <div class="title">
          <h2>{card.metadata.name}</h2>
          <span class="kind"><span aria-hidden="true">●</span> {card.kind}</span>
          <label class="version">
            <span class="visually-hidden">Version</span>
            <!-- The selection always shows the version the host passed; a chosen one shows once the host renders it. -->
            <select
              disabled={!onaction || versions?.state !== 'populated' || blocked(versionState)}
              aria-busy={versionState?.state === 'pending' ? 'true' : undefined}
              value={input.cardRef.version}
              onchange={(event) => {
                onaction?.({ name: 'card.version', input: { card_ref: { ...ref, version: event.currentTarget.value } } });
                event.currentTarget.value = input.cardRef.version;
              }}
            >
              {#each items as item (item.version)}
                <option value={item.version}>v{item.version}{item.version === newest ? ' · current' : ''}</option>
              {/each}
              {#if !items.some((item) => item.version === input.cardRef.version)}
                <option value={input.cardRef.version}>v{input.cardRef.version}</option>
              {/if}
            </select>
          </label>
          <ActionNote state={versionState} />
          {#if current}<span class="status"><StatusPill tone={TONES[current.status]} label={current.status} /></span>{/if}
        </div>
        <p class="meta">
          <span>{fmtCardRef(ref)}</span>
          {#if card.metadata.uid}
            <span>
              uid {card.metadata.uid}
              <button type="button" class="icon" aria-label="Copy uid" onclick={() => copy(card.metadata.uid ?? '')}>⧉</button>
            </span>
          {/if}
          {#if current}
            <span>registered <time datetime={current.created_at}>{fmtInstant(parseInstant(current.created_at))}</time></span>
          {/if}
          {#if versions?.state === 'populated' && versions.data.truncated}
            <span>showing the newest {items.length} versions</span>
          {:else if versions?.state === 'denied' || versions?.state === 'error'}
            <span class="problem" role="note">versions unavailable: {versions.problem.title} ({versions.problem.code})</span>
          {/if}
          {#if copied}<span role="status">{copied}</span>{/if}
        </p>
      </div>
      <div class="actions">
        <ActionButton label="Copy ref" lead="⧉" onclick={() => copy(fmtCardRef(ref))} />
        <ActionButton
          variant="primary"
          label="Open in Observe"
          icon="↗"
          state={onaction ? actions['card.open_in_observe'] : unavailable}
          onclick={() => onaction?.({ name: 'card.open_in_observe', input: { card_ref: ref } })}
        />
      </div>
    </div>

    {#if input.tabs?.length}
      <nav aria-label="{card.metadata.name} sections">
        {#each input.tabs as tab (tab.id)}
          <button
            type="button"
            aria-current={tab.id === (input.tab ?? input.tabs[0].id) ? 'page' : undefined}
            disabled={!onaction || blocked(tabState)}
            aria-busy={tabState?.state === 'pending' ? 'true' : undefined}
            onclick={() => onaction?.({ name: 'card.tab', input: { tab: tab.id } })}>{tab.label}</button
          >
        {/each}
        <ActionNote state={tabState} />
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
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
    border-bottom: 1px solid var(--border);
    color: var(--text);
    font-family: var(--font-sans);
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 12px 16px;
  }
  .identity {
    display: flex;
    flex: 1 1 320px;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  h2 {
    margin: 0;
    min-width: 0;
    font: 600 28px/1.15 var(--font-sans);
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }
  .kind {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid color-mix(in srgb, var(--declare) 45%, var(--border));
    border-radius: var(--r);
    padding: 2px 8px;
    background: var(--declare-soft);
    color: var(--declare);
    font: 11px var(--font-mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .version {
    position: relative;
    display: inline-flex;
  }
  .version::after {
    content: '▾';
    position: absolute;
    top: 50%;
    right: 9px;
    color: var(--muted);
    font-size: 10px;
    pointer-events: none;
    transform: translateY(-50%);
  }
  select {
    appearance: none;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 3px 24px 3px 9px;
    background: var(--surface-2);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  select:enabled {
    cursor: pointer;
  }
  select:enabled:hover {
    background: var(--surface-hover);
  }
  .status :global(.pill) {
    text-transform: uppercase;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 14px;
    margin: 0;
    color: var(--muted);
    font: 11.5px var(--font-mono);
  }
  .meta span {
    overflow-wrap: anywhere;
  }
  .problem {
    color: var(--warn);
  }
  .icon {
    border: 0;
    padding: 0 2px;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  .icon:hover {
    color: var(--declare);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: flex-end;
    gap: 8px;
    margin-left: auto;
  }
  nav {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 22px;
  }
  nav button {
    border: 0;
    padding: 10px 2px;
    background: none;
    color: var(--muted);
    font: 13px var(--font-sans);
    cursor: pointer;
  }
  nav button:enabled:hover {
    color: var(--text);
  }
  nav button[aria-current='page'] {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--declare);
  }
  nav button:disabled {
    cursor: default;
  }
  :is(select, button):focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
