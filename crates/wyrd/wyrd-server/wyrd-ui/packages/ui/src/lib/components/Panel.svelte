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
    gap: 4px 10px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  h3 {
    margin: 0;
    font: 600 11px var(--font-mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .note {
    color: var(--muted);
    font: 11px var(--font-mono);
  }
  .tools {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-left: auto;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    padding: 12px 14px;
  }
  .body :global(table) {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-mono);
  }
  .body :global(th) {
    height: 28px;
    padding: 0 10px 0 0;
    border-bottom: 1px solid var(--border-soft);
    color: var(--muted);
    font: 11px var(--font-mono);
    letter-spacing: 0.06em;
    text-align: left;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .body :global(td) {
    height: 34px;
    max-width: 360px;
    padding: 0 10px 0 0;
    border-bottom: 1px solid var(--border-soft);
    overflow-wrap: anywhere;
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
