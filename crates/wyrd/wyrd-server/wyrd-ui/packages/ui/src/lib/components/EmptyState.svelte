<!--
  The one place a component says there is nothing to draw and why. The state
  word is always written beside its glyph; `code` is the server's stable Wyrd
  error code verbatim.
-->
<script lang="ts">
  import type { Problem } from '../contract.js';

  interface Props {
    state: 'loading' | 'empty' | 'denied' | 'error';
    /** What is missing or failed; defaults to the problem's title. */
    title?: string;
    detail?: string;
    problem?: Problem;
  }

  let { state, title, detail, problem }: Props = $props();

  const WORDS = { loading: 'Loading', empty: 'Empty', denied: 'Not authorized', error: 'Error' } as const;
  const GLYPHS = { loading: '●', empty: '○', denied: '!', error: '✕' } as const;
</script>

<div
  class="state"
  data-state={state}
  role={state === 'loading' ? 'status' : 'note'}
  aria-busy={state === 'loading' ? 'true' : undefined}
>
  <span class="word"><span aria-hidden="true">{GLYPHS[state]}</span> {WORDS[state]}</span>
  {#if title ?? problem?.title}<strong>{title ?? problem?.title}</strong>{/if}
  {#if detail ?? problem?.detail}<p>{detail ?? problem?.detail}</p>{/if}
  {#if problem}<code>{problem.code}</code>{/if}
</div>

<style>
  .state {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    border: 1px dashed var(--border);
    border-radius: var(--r);
    padding: 22px 16px;
    color: var(--muted);
    font: 12px/1.5 var(--font-mono);
    overflow-wrap: anywhere;
  }
  .state[data-state='error'] {
    border-color: var(--danger);
  }
  .state[data-state='denied'] {
    border-color: var(--warn);
  }
  .word {
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  strong {
    color: var(--text);
    font: 600 13px var(--font-sans);
  }
  p {
    margin: 0;
  }
  code {
    font: inherit;
  }
</style>
