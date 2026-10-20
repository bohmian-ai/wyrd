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
    /** `primary` is the one strong action on a surface; `link` sits in a panel head. */
    variant?: 'primary' | 'secondary' | 'link';
    /** A decorative glyph before the label, such as ⧉ for copy; hidden from assistive tech. */
    lead?: string;
    /** A decorative glyph after the label, such as ↗ for navigation away; hidden from assistive tech. */
    icon?: string;
    disabled?: boolean;
    onclick: () => void;
  }

  let { label, state, variant = 'secondary', lead, icon, disabled = false, onclick }: Props = $props();

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
  <button type="button" class={variant} disabled={blocked} aria-busy={state?.state === 'pending' ? 'true' : undefined} {onclick}>
    {#if lead}<span class="lead" aria-hidden="true">{lead}</span>{/if}{state?.state === 'pending' ? `${label}…` : label}{#if icon}<span class="icon" aria-hidden="true">{icon}</span>{/if}
  </button>
  {#if note}<span class="note" data-state={state?.state} role={state?.state === 'success' ? 'status' : 'alert'}>{note}</span>{/if}
</span>

<style>
  .action {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 4px 8px;
    min-width: 0;
  }
  button {
    display: inline-flex;
    align-items: center;
    height: 34px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    padding: 0 12px;
    background: var(--surface-2);
    color: var(--text);
    font: 13px var(--font-sans);
    white-space: nowrap;
    cursor: pointer;
  }
  .secondary:hover:not(:disabled) {
    background: var(--surface-hover);
  }
  .primary {
    border-color: var(--declare);
    background: var(--declare);
    color: var(--declare-ink);
    font-weight: 600;
  }
  .primary:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--declare) 82%, var(--text));
    background: color-mix(in srgb, var(--declare) 82%, var(--text));
  }
  .lead {
    margin-right: 8px;
  }
  .icon {
    margin-left: 6px;
  }
  .link .icon {
    margin-left: 4px;
  }
  .link {
    height: auto;
    border: 0;
    padding: 0;
    background: none;
    color: var(--declare);
    font: 11px var(--font-mono);
  }
  .link:hover:not(:disabled) {
    text-decoration: underline;
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  button:focus-visible {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
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
