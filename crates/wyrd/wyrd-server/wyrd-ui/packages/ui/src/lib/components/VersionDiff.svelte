<!--
  The authored differences between two registered versions of one Prompt,
  from the other version to the selected one: each changed field side by
  side under its version, removed lines marked on the left and added lines
  on the right. Choosing the other version is a host action.
-->
<script lang="ts">
  import type { ActionProps } from '../contract.js';
  import type { VersionDiffProps } from '../cards.js';
  import { diffLines, fmtYaml } from '../format.js';
  import ActionNote from './ActionNote.svelte';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result, actions = {}, onaction }: VersionDiffProps & ActionProps = $props();

  const state = $derived(actions['prompt.compare']);
  const data = $derived(result.state === 'populated' || result.state === 'empty' ? result.data : undefined);
  const others = $derived(data?.versions.filter((version) => version !== input.cardRef.version) ?? []);
  const show = (value: unknown) => (typeof value === 'string' ? value : fmtYaml(value));
  /** One field's two sides; a side the field is absent from has no lines. */
  const sides = (change: { from?: unknown; to?: unknown }) => {
    if (change.from === undefined || change.to === undefined) {
      const lines = (value: unknown) => (value === undefined ? [] : show(value).split('\n').map((text) => ({ kind: 'changed' as const, text })));
      return { from: lines(change.from), to: lines(change.to) };
    }
    return diffLines(show(change.from), show(change.to));
  };
</script>

<Panel
  title={data?.other ? `Changes from v${data.other}` : 'Changes between versions'}
  note={data?.other ? `v${data.other} → v${input.cardRef.version}` : 'compares any two versions'}
>
  {#snippet tools()}
    {#if others.length > 0 && data}
      <label>
        <span class="visually-hidden">Compare with</span>
        <select
          value={data.other ?? ''}
          disabled={!onaction || state?.state === 'pending' || state?.state === 'unavailable'}
          onchange={(event) => {
            onaction?.({ name: 'prompt.compare', input: { other_version: event.currentTarget.value } });
            event.currentTarget.value = data.other ?? '';
          }}
        >
          {#if !data.other}<option value="" disabled>compare with…</option>{/if}
          {#each others as version (version)}<option value={version}>compare with v{version}</option>{/each}
        </select>
      </label>
      <ActionNote {state} />
    {/if}
  {/snippet}
  {#if result.state === 'populated'}
    {#each result.data.changes as change (change.field)}
      {@const lines = sides(change)}
      <section aria-label={change.field}>
        <h4>{change.field}</h4>
        <div class="sides">
          {@render side('from', `v${result.data.other}`, lines.from)}
          {@render side('to', `v${input.cardRef.version}`, lines.to)}
        </div>
      </section>
    {/each}
  {:else if result.state === 'empty'}
    <EmptyState
      state="empty"
      title={result.reason}
      detail={others.length === 0 ? "Registering a new version shows its message, variable and response-format diff here." : undefined}
    />
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Comparing {input.cardRef.name} versions" />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</Panel>

{#snippet side(which: 'from' | 'to', version: string, lines: ReturnType<typeof sides>['from'])}
  <div class="side">
    <span class="version">{version}</span>
    <pre data-side={which} aria-label={version}>{#each lines as line, index (index)}<span data-line={line.kind}>{line.kind === 'changed' ? (which === 'from' ? '− ' : '+ ') : '  '}{line.text}
</span>{:else}<span data-line="gap">not set</span>{/each}</pre>
  </div>
{/snippet}

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
  .side {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .version {
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  pre {
    flex: 1;
  }
  [data-line] {
    display: block;
    border-left: 2px solid transparent;
    padding-left: 6px;
  }
  [data-line='same'],
  [data-line='gap'] {
    color: var(--muted);
  }
  [data-line='gap'] {
    font-style: italic;
  }
  [data-side='from'] [data-line='changed'] {
    border-left-color: var(--danger);
    background: var(--danger-soft);
    color: var(--danger);
  }
  [data-side='to'] [data-line='changed'] {
    border-left-color: var(--ok);
    color: var(--ok);
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
