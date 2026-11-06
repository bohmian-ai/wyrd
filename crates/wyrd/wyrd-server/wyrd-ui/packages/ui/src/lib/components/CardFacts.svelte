<!--
  An exact Card's server-managed facts, or, with `yaml`, the whole Card as
  the server returned it. Nothing here is editable.
-->
<script lang="ts">
  import type { CardFactsProps } from '../cards.js';
  import { fmtInstant, fmtShortId, fmtYaml, parseInstant } from '../format.js';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result }: CardFactsProps = $props();

  const card = $derived(result.state === 'populated' ? result.data : undefined);
  const facts = $derived.by(() => {
    if (!card) return [];
    const metadata = card.metadata;
    const labels = Object.entries(metadata.labels ?? {}).map(([key, value]) => `${key}=${value}`);
    // The header already carries the uid and lifecycle phase.
    const rows: [string, string | undefined, boolean?][] = [
      ['labels', labels.length > 0 ? labels.join(', ') : undefined],
      ...Object.entries(metadata.annotations ?? {}).map(([key, value]): [string, string] => [`annotations.${key}`, typeof value === 'string' ? value : JSON.stringify(value)]),
      ['spec hash', metadata.spec_hash ?? undefined, true],
      ['artifact hash', metadata.artifact_hash ?? undefined, true],
      ['message', card.status?.message ?? undefined],
      ['updated', card.status?.updated_at ? fmtInstant(parseInstant(card.status.updated_at)) : undefined],
      ['origin', metadata.origin ? JSON.stringify(metadata.origin) : undefined]
    ];
    return rows.filter(([term, value]) => term === 'labels' || value !== undefined);
  });
</script>

{#if card && input.yaml}
  <Panel title="YAML" note="the Card as the server returned it">
    <pre>{fmtYaml(card)}</pre>
  </Panel>
{:else if card}
  <Panel title="Metadata" note="server-managed">
    <dl>
      {#each facts as [term, value, hash] (term)}
        <dt>{term}</dt>
        {#if value === undefined}
          <dd class="muted">none</dd>
        {:else if hash}
          <dd title={value}>{fmtShortId(value)} <button type="button" aria-label="Copy {term}" onclick={() => navigator.clipboard.writeText(value).catch(() => {})}>⧉</button></dd>
        {:else}
          <dd title={value}>{value}</dd>
        {/if}
      {/each}
    </dl>
  </Panel>
{:else if result.state === 'loading'}
  <EmptyState state="loading" title="Loading {input.cardRef.name}@{input.cardRef.version}" />
{:else if result.state === 'denied' || result.state === 'error'}
  <EmptyState state={result.state} problem={result.problem} />
{/if}

<style>
  dl {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: 8px 12px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  button {
    border: 0;
    padding: 0 2px;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }
  button:hover {
    color: var(--declare);
  }
  button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
</style>
