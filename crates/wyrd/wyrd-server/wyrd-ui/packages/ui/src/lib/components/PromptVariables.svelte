<!-- The variables the server's Prompt compiler derived from an exact Prompt. -->
<script lang="ts">
  import type { PromptVariablesProps } from '../cards.js';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result }: PromptVariablesProps = $props();
</script>

<Panel title="Variables" note="derived from the messages">
  {#if result.state === 'populated'}
    <table>
      <thead><tr><th>Name</th><th>Kind</th></tr></thead>
      <tbody>
        {#each result.data as variable (`${variable.kind}:${variable.name}`)}
          <tr><td>{variable.name}</td><td class="muted">{variable.kind}</td></tr>
        {/each}
      </tbody>
    </table>
  {:else if result.state === 'empty'}
    <EmptyState state="empty" title={result.reason} />
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading variables of {input.cardRef.name}" />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</Panel>
