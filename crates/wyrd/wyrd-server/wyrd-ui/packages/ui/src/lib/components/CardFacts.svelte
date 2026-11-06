<!--
  An exact Card's server-managed facts, or, with `yaml`, the whole Card as
  the server returned it. Nothing here is editable.
-->
<script lang="ts">
  import type { CardFactsProps } from '../cards.js';
  import { fmtInstant, fmtYaml, parseInstant } from '../format.js';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result }: CardFactsProps = $props();

  const card = $derived(result.state === 'populated' ? result.data : undefined);
  const facts = $derived.by(() => {
    if (!card) return [];
    const metadata = card.metadata;
    const rows: [string, string | undefined][] = [
      ['uid', metadata.uid ?? undefined],
      ['phase', card.status?.phase],
      ['message', card.status?.message ?? undefined],
      ['spec hash', metadata.spec_hash ?? undefined],
      ['artifact hash', metadata.artifact_hash ?? undefined],
      ['updated', card.status?.updated_at ? fmtInstant(parseInstant(card.status.updated_at)) : undefined],
      ...Object.entries(metadata.labels ?? {}).map(([key, value]): [string, string] => [`labels.${key}`, String(value)]),
      ...Object.entries(metadata.annotations ?? {}).map(([key, value]): [string, string] => [`annotations.${key}`, typeof value === 'string' ? value : JSON.stringify(value)]),
      ['origin', metadata.origin ? JSON.stringify(metadata.origin) : undefined]
    ];
    return rows.filter((row): row is [string, string] => row[1] !== undefined);
  });
</script>

{#if card && input.yaml}
  <Panel title="YAML" note="the Card as the server returned it">
    <pre>{fmtYaml(card)}</pre>
  </Panel>
{:else if card}
  <Panel title="Metadata" note="server-managed">
    <dl>
      {#each facts as [term, value] (term)}
        <dt>{term}</dt>
        <dd>{value}</dd>
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
    grid-template-columns: minmax(90px, max-content) minmax(0, 1fr);
    gap: 6px 14px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
