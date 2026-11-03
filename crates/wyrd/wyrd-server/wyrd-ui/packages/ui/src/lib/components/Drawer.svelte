<!--
  A side panel on the native modal <dialog>: focus trapping, Escape and the
  backdrop come from the platform, and a click on the backdrop closes it like
  Escape. The host owns `open`; closing reports through `onclose`.
  A host may widen it with `width` (never past the viewport), replace the
  title text with a `heading` (the title stays its accessible name) and add
  header `tools` before Close.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    open: boolean;
    title: string;
    onclose: () => void;
    children: Snippet;
    width?: string;
    heading?: Snippet;
    tools?: Snippet;
  }

  let { open, title, onclose, children, width = '480px', heading, tools }: Props = $props();
  let dialog: HTMLDialogElement | undefined = $state();

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
  /** A backdrop click lands on the dialog itself but outside its box. */
  function backdrop(event: MouseEvent) {
    const box = dialog?.getBoundingClientRect();
    if (event.target === dialog && box && (event.clientX < box.left || event.clientX > box.right || event.clientY < box.top || event.clientY > box.bottom)) onclose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -- Escape is the keyboard equivalent. -->
<dialog bind:this={dialog} aria-label={title} {onclose} onclick={backdrop} style:width="min({width}, 100vw)">
  <header>
    <div class="heading">{#if heading}{@render heading()}{:else}<h2>{title}</h2>{/if}</div>
    <span class="tools">
      {@render tools?.()}
      <button type="button" aria-label="Close {title}" onclick={onclose}>✕ <span class="esc" aria-hidden="true">Esc</span></button>
    </span>
  </header>
  <div class="body">{@render children()}</div>
</dialog>

<style>
  dialog {
    inset: 0 0 0 auto;
    height: 100dvh;
    max-width: 100vw;
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
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    border-bottom: 1px solid var(--border-soft);
    padding: 12px 16px;
  }
  .heading {
    min-width: 0;
  }
  h2 {
    margin: 0;
    font: var(--weight-strong) 14px var(--font-sans);
  }
  .tools {
    display: flex;
    flex-shrink: 0;
    gap: 6px;
  }
  .tools :global(button),
  button {
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    font: 12px var(--font-sans);
    cursor: pointer;
  }
  .esc {
    color: var(--muted);
  }
  .body {
    padding: 16px;
    overflow: auto;
  }
</style>
