<!--
  One typed action control. The host reports each action's state: `pending`
  disables the control so an intent is never sent twice, and a refusal or
  failure stays visible beside it until the next attempt.
-->
<script lang="ts">
  import type { ActionState } from '../contract.js';

  interface Props {
    label: string;
    state?: ActionState;
    primary?: boolean;
    disabled?: boolean;
    onclick: () => void;
  }

  let { label, state, primary = false, disabled = false, onclick }: Props = $props();

  const blocked = $derived(disabled || state?.state === 'pending' || state?.state === 'unavailable');
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

<span class="action">
  <button type="button" class:primary disabled={blocked} aria-busy={state?.state === 'pending' ? 'true' : undefined} {onclick}>
    {state?.state === 'pending' ? `${label}…` : label}
  </button>
  {#if note}<span class="note" data-state={state?.state} role={state?.state === 'success' ? 'status' : 'alert'}>{note}</span>{/if}
</span>

<style>
  .action {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  button {
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 4px 10px;
    background: var(--surface-2);
    color: var(--text);
    font: var(--weight-strong) 12px var(--font-sans);
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--surface-hover);
  }
  button.primary {
    border-color: var(--declare);
    background: var(--declare);
    color: var(--declare-ink);
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }
  button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .note {
    font: 11px var(--font-sans);
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .note[data-state='denied'] {
    color: var(--warn);
  }
  .note[data-state='error'] {
    color: var(--danger);
  }
</style>
