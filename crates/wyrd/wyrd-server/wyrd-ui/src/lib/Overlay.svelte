<!--
  An administration drawer or confirmation dialog whose open state lives in
  the URL: the page renders it when its query parameter is present, and the
  scrim, the close control and Escape all go to `close`, which writes nothing.
  Styled by console.css; focus moves into it when it opens.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { goto } from '$app/navigation';

  interface Props {
    kind: 'drawer' | 'dialog';
    /** The accessible name, and the drawer's heading. */
    title: string;
    /** Where closing goes; the page without this overlay's parameter. */
    close: string;
    /** A drawer heading richer than its title. */
    heading?: Snippet;
    children: Snippet;
    footer?: Snippet;
  }
  let { kind, title, close, heading, children, footer }: Props = $props();
  const uid = $props.id();

  /** Focus the overlay itself once, so keyboard and screen-reader users start inside it. */
  const focus = (node: HTMLElement) => node.focus();
  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') goto(close, { noScroll: true, keepFocus: true });
  }
</script>

<svelte:window {onkeydown} />
<a class="scrim" href={close} aria-label="Close" data-sveltekit-noscroll></a>
{#if kind === 'drawer'}
  <div class="drawer" aria-label={title} role="dialog" aria-modal="true" tabindex="-1" use:focus>
    <header>
      {#if heading}{@render heading()}{:else}<h2>{title}</h2>{/if}
      <a class="iconbtn" href={close} aria-label="Close" data-sveltekit-noscroll>✕</a>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
{:else}
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="{uid}-title" tabindex="-1" use:focus>
    <div class="body">
      <h2 id="{uid}-title">{title}</h2>
      {@render children()}
    </div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
{/if}
