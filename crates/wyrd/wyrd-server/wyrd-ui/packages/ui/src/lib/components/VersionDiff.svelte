<!--
  The authored differences between two registered versions of one Prompt,
  from the other version to the selected one. Choosing the other version is
  a host action.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import type { VersionDiffProps } from '../cards.js';
  import { fmtYaml } from '../format.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result, actions = {}, onaction }: VersionDiffProps & ActionProps = $props();

  const state = $derived(actions['prompt.compare']);
  const data = $derived(result.state === 'populated' || result.state === 'empty' ? result.data : undefined);
  const others = $derived(data?.versions.filter((version) => version !== input.cardRef.version) ?? []);
  const show = (value: unknown) => (value === undefined ? '(unset)' : typeof value === 'string' ? value : fmtYaml(value));
</script>

<Panel title="Changes" note={data?.other ? `v${data.other} → v${input.cardRef.version}` : 'between versions'}>
  {#snippet tools()}
    {#if others.length > 0 && data?.other}
      <label>
        <span class="visually-hidden">Compare with</span>
        <select
          value={data.other}
          disabled={!onaction || state?.state === 'pending' || state?.state === 'unavailable'}
          onchange={(event) => {
            onaction?.({ name: 'prompt.compare', input: { other_version: event.currentTarget.value } });
            event.currentTarget.value = data.other ?? '';
          }}
        >
          {#each others as version (version)}<option value={version}>compare with v{version}</option>{/each}
        </select>
      </label>
      <ActionNote {state} />
    {/if}
  {/snippet}
  {#if result.state === 'populated'}
    {#each result.data.changes as change (change.field)}
      <section aria-label={change.field}>
        <h4>{change.field}</h4>
        <div class="sides">
          <pre data-side="from" aria-label="v{result.data.other}">{show(change.from)}</pre>
          <pre data-side="to" aria-label="v{input.cardRef.version}">{show(change.to)}</pre>
        </div>
      </section>
    {/each}
  {:else if result.state === 'empty'}
    <EmptyState state="empty" title={result.reason} />
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Comparing {input.cardRef.name} versions" />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</Panel>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h4 {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
  }
  .sides {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 8px;
  }
  [data-side='from'] {
    border-left: 2px solid var(--border);
  }
  [data-side='to'] {
    border-left: 2px solid var(--declare);
  }
  select {
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 8px;
    background: var(--surface-2);
    color: var(--text);
    font: 12px var(--font-mono);
  }
  select:focus-visible {
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
