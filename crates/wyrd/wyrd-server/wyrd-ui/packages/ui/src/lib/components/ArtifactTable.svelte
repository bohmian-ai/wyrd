<!--
  The files registration recorded for one exact Card version, as stored:
  relative path, size, SHA-256 digest and content type.
-->
<script lang="ts">
  import type { ArtifactTableProps } from '../cards.js';
  import { fmtBytes } from '../format.js';
  import EmptyState from './EmptyState.svelte';
  import Panel from './Panel.svelte';

  let { input, result }: ArtifactTableProps = $props();
</script>

<Panel title="Files" note="recorded at registration">
  {#if result.state === 'populated'}
    <table>
      <thead><tr><th>Path</th><th>Size</th><th>SHA-256</th><th>Content type</th></tr></thead>
      <tbody>
        {#each result.data as file (file.relative_path)}
          <tr>
            <td>{file.relative_path}</td>
            <td title="{file.size_bytes} bytes">{fmtBytes(file.size_bytes)}</td>
            <td class="muted">{file.sha256}</td>
            <td>{file.content_type ?? '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if result.state === 'empty'}
    <EmptyState state="empty" title={result.reason} />
  {:else if result.state === 'loading'}
    <EmptyState state="loading" title="Loading files of {input.cardRef.name}@{input.cardRef.version}" />
  {:else}
    <EmptyState state={result.state} problem={result.problem} />
  {/if}
</Panel>
