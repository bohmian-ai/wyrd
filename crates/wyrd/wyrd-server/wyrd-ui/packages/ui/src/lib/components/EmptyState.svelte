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
    display: grid;
    gap: 4px;
    border: 1px dashed var(--muted);
    border-radius: var(--r);
    padding: 14px;
    color: var(--text);
    font: 12px/18px var(--font-sans);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .state[data-state='error'] {
    border-color: var(--danger);
  }
  .state[data-state='denied'] {
    border-color: var(--warn);
  }
  .word {
    color: var(--muted);
    font: var(--weight-strong) 11px var(--font-mono);
    text-transform: uppercase;
  }
  strong {
    font-size: 13px;
  }
  p {
    margin: 0;
    color: var(--muted);
  }
  code {
    font: 11px var(--font-mono);
    color: var(--muted);
  }
</style>
