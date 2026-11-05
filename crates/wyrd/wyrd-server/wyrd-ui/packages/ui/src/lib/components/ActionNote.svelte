<!--
  The visible outcome of one host action beside its control: a refusal or
  failure with its stable code, an unavailable reason, or a success message.
  Pending shows nothing here; the control itself reports it.
-->
<script lang="ts">
  import type { ActionState } from '../contract.js';

  let { state }: { state?: ActionState } = $props();

  const note = $derived.by(() => {
    switch (state?.state) {
      case 'denied':
      case 'error':
        return `${state.problem.title} (${state.problem.code})`;
      case 'unavailable':
        return state.reason;
      case 'success':
        return state.message;
      default:
        return undefined;
    }
  });
</script>

{#if note}<span class="note" data-state={state?.state} role={state?.state === 'success' ? 'status' : 'alert'}>{note}</span>{/if}

<style>
  .note {
    color: var(--muted);
    font: 11px var(--font-mono);
    overflow-wrap: anywhere;
  }
  .note[data-state='denied'] {
    color: var(--warn);
  }
  .note[data-state='error'] {
    color: var(--danger);
  }
</style>
