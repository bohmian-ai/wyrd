<!--
  The authored differences between two registered versions of one Prompt,
  from the other version to the selected one, as a unified diff: each
  changed field is a section counting its added and removed lines, removed
  lines above added ones. A large change starts closed. Choosing the other
  version is a host action.
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
  const show = (value: unknown) => (value === undefined ? undefined : typeof value === 'string' ? value : fmtYaml(value));
  /** Changed lines above which a field starts closed. */
  const OPEN_LINES = 12;
  const fields = $derived(
    (result.state === 'populated' ? result.data.changes : []).map((change) => {
      const lines = diffLines(show(change.from), show(change.to));
      const count = (kind: string) => lines.filter((line) => line.kind === kind).length;
      return { field: change.field, lines, added: count('added'), removed: count('removed') };
    })
  );
  const MARK = { same: ' ', gap: ' ', removed: '−', added: '+' } as const;
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
    {#each fields as each (each.field)}
      <details aria-label={each.field} open={each.added + each.removed <= OPEN_LINES}>
        <summary>
          <span class="field">{each.field}</span>
          {#if each.added}<span data-count="added">+{each.added}</span>{/if}
          {#if each.removed}<span data-count="removed">−{each.removed}</span>{/if}
        </summary>
        <pre>{#each each.lines as line, index (index)}<span data-line={line.kind}>{MARK[line.kind]} {line.text}
</span>{/each}</pre>
      </details>
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

<style>
  summary {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 2px 0 6px;
    font: 12px var(--font-mono);
    cursor: pointer;
  }
  /* A flex summary drops the native marker, so the open state is drawn here. */
  summary::before {
    content: '▸';
    color: var(--muted);
  }
  details[open] > summary::before {
    content: '▾';
  }
  summary:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .field {
    color: var(--text);
  }
  [data-count='added'],
  [data-line='added'] {
    color: var(--ok);
  }
  [data-count='removed'],
  [data-line='removed'] {
    color: var(--danger);
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
  [data-line='removed'] {
    border-left-color: var(--danger);
    background: var(--danger-soft);
  }
  [data-line='added'] {
    border-left-color: var(--ok);
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
