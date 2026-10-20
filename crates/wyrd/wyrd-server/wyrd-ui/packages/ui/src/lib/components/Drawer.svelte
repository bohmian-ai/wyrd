<!--
  A side panel on the native modal <dialog>: focus trapping, Escape and the
  backdrop come from the platform. The host owns `open`; closing reports through `onclose`.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open: boolean;
    title: string;
    onclose: () => void;
    children: Snippet;
  }

  let { open, title, onclose, children }: Props = $props();
  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog bind:this={dialog} aria-label={title} {onclose}>
  <header>
    <h2>{title}</h2>
    <button type="button" aria-label="Close {title}" onclick={onclose}>✕</button>
  </header>
  <div class="body">{@render children()}</div>
</dialog>

<style>
  dialog {
    inset: 0 0 0 auto;
    width: min(480px, 100vw);
    height: 100dvh;
    max-height: none;
    margin: 0;
    border: 0;
    border-left: 1px solid var(--border);
    padding: 0;
    background: var(--surface);
    color: var(--text);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    border-bottom: 1px solid var(--border-soft);
    padding: 12px 16px;
  }
  h2 {
    margin: 0;
    font: var(--weight-strong) 14px var(--font-sans);
  }
  button {
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .body {
    padding: 16px;
    overflow: auto;
  }
</style>
