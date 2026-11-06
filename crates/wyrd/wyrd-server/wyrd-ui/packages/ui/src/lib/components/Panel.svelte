<!--
  One ruled panel: an uppercase title, an optional muted note saying where its
  facts come from, optional head controls, then the body. Tables, code blocks
  and row buttons inside a panel share these rules.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    note?: string;
    tools?: Snippet;
    children: Snippet;
  }

  let { title, note, tools, children }: Props = $props();
</script>

<section class="panel" aria-label={title}>
  <header>
    <h3>{title}</h3>
    {#if note}<span class="note">{note}</span>{/if}
    {#if tools}<span class="tools">{@render tools()}</span>{/if}
  </header>
  <div class="body">{@render children()}</div>
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font-family: var(--font-sans);
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
  }
  h3 {
    margin: 0;
    color: var(--muted);
    font: 400 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .note {
    margin-left: auto;
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .tools {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-left: auto;
  }
  .note + .tools {
    margin-left: 0;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
    padding: 14px 16px;
  }
  /* A table or `.flush` list runs edge to edge; anything beside it keeps the body inset. */
  .body:has(> :global(:is(table, .flush))) {
    gap: 0;
    padding: 0;
  }
  .body:has(> :global(:is(table, .flush))) > :global(:not(table, .flush)) {
    margin: 10px 16px;
  }
  .body :global(table) {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-mono);
  }
  .body :global(th) {
    padding: 8px 16px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font: 400 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-align: left;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .body :global(td) {
    max-width: 360px;
    padding: 9px 16px;
    border-bottom: 1px solid var(--border-soft);
    overflow-wrap: anywhere;
  }
  .body :global(tr:last-child td) {
    border-bottom: 0;
  }
  .body :global(pre) {
    margin: 0;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
    padding: 10px 12px;
    background: var(--surface-2);
    font: 12px/1.55 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .body :global(.open) {
    border: 0;
    padding: 0;
    background: none;
    color: var(--declare);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .body :global(.open:hover:enabled) {
    text-decoration: underline;
  }
  .body :global(.open:disabled) {
    color: var(--text);
    cursor: default;
  }
  .body :global(:is(button, select, input):focus-visible) {
    outline: 2px solid var(--declare);
    outline-offset: 2px;
  }
  .body :global(.muted) {
    color: var(--muted);
  }
</style>
