<script lang="ts">
  // C-12 — the earned Verifier presentation: purpose, accepted normalized
  // Evidence kinds, capabilities and runtime binding. No secrets render, and
  // the input world stays open — declared, not exhaustive.
  import Panel from '$lib/components/Panel.svelte';
  import Table from '$lib/components/Table.svelte';
  import type { CardDetail, VerifierPresentation } from '../../core/types';

  let { detail }: { detail: CardDetail; base: string } = $props();
  const spec = $derived(detail.presentation as VerifierPresentation);
</script>

<Panel title="Purpose" variant="raised">
  <p>{spec.purpose}</p>
</Panel>
<Panel title="Accepted evidence" variant="raised">
  {#snippet head()}<span class="mono muted">content-addressed</span>{/snippet}
  <Table label="Accepted evidence kinds">
    <table>
      <thead><tr><th>Kind</th><th>Required</th><th>Binding</th></tr></thead>
      <tbody>
        {#each spec.evidence as row (row.kind)}
          <tr>
            <td class="mono"><strong>{row.kind}</strong></td>
            <td class="mono">{row.required}</td>
            <td class="mono">{row.binding}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </Table>
  <p class="muted">{spec.evidenceNote}</p>
</Panel>
<Panel title="Capabilities &amp; runtime" variant="raised">
  <div class="kv">
    {#each spec.capabilities as entry (entry.k)}
      <span class="k">{entry.k}</span><span class="mono">{entry.v}</span>
    {/each}
  </div>
</Panel>
<div class="notice">
  <p class="mono muted">SECRETS &amp; CLOSED INPUTS</p>
  <p class="muted">{spec.secrets}</p>
</div>
