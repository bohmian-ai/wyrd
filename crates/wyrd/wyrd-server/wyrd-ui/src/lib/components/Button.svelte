<script lang="ts">
  import type { Snippet } from 'svelte';

  // One control surface for both actions and navigation. `href` renders an <a> so a
  // catalog-safe view can offer navigation without handing a callback across the
  // contract; the styling, press feedback and geometry are identical either way.
  type Variant = 'primary' | 'secondary' | 'ghost';
  let {
    variant = 'primary',
    href,
    children
  }: {
    variant?: Variant;
    href?: string;
    children?: Snippet;
  } = $props();
</script>

{#if href}
  <a {href} class="wy-btn" data-variant={variant}>{@render children?.()}</a>
{:else}
  <button type="button" class="wy-btn" data-variant={variant}>{@render children?.()}</button>
{/if}

<style>
  .wy-btn {
    display: inline-block;
    font-family: var(--fm);
    font-size: 13px;
    font-weight: var(--weight-strong);
    text-decoration: none;
    padding: 8px 13px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
    transition:
      transform 0.04s,
      box-shadow 0.04s;
  }
  .wy-btn:hover {
    transform: translate(-1px, -1px);
  }
  .wy-btn:active {
    transform: translate(2px, 2px);
  }
  /* an explicit ring stays visible on the declare fill */
  .wy-btn:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  .wy-btn[data-variant='primary'] {
    background: var(--declare);
    color: var(--declare-ink);
  }
  .wy-btn[data-variant='secondary'] {
    background: var(--surface-2);
    color: var(--text);
  }
  .wy-btn[data-variant='ghost'] {
    background: var(--surface);
    color: var(--text);
  }
</style>
