<script lang="ts">
  // C-08 — the earned Workflow presentation: ordered stages dominant, then
  // inputs/outputs and governance. A Workflow is a declaration — execution
  // and results stay in Observe, and the absent section says so in place.
  import Panel from '$lib/components/Panel.svelte';
  import type { CardDetail, WorkflowPresentation } from '../../core/types';

  let { detail, base }: { detail: CardDetail; base: string } = $props();
  const spec = $derived(detail.presentation as WorkflowPresentation);
</script>

<Panel title="Stages" variant="raised">
  {#snippet head()}<span class="mono muted">ordered · {spec.stages.length}</span>{/snippet}
  <div class="stages">
    {#each spec.stages as stage, i (i)}
      {#if i > 0}<span class="arrow" aria-hidden="true">→</span>{/if}
      <span class="stage">{stage}</span>
    {/each}
  </div>
  <p class="muted">{spec.stageNote}</p>
</Panel>
<Panel title="Inputs &amp; outputs" variant="raised">
  <div class="kv">
    {#each spec.io as entry (entry.k)}
      <span class="k">{entry.k}</span>
      {#if entry.href}<span class="mono"><a href={base + entry.href}>{entry.v}</a></span>
      {:else}<span class="mono">{entry.v}</span>{/if}
    {/each}
  </div>
</Panel>
<Panel title="Governance" variant="raised">
  <div class="kv">
    {#each spec.governance as entry (entry.k)}
      <span class="k">{entry.k}</span><span class="mono">{entry.v}</span>
    {/each}
  </div>
</Panel>
<div class="notice">
  <p class="mono muted">EXECUTION RESULTS</p>
  <p class="muted">{spec.execution}</p>
</div>
