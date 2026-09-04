<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';

  // One control surface for both actions and navigation. `href` renders an <a> so a
  // catalog-safe view can offer navigation without handing a callback across the
  // contract; the styling, press feedback and geometry are identical either way.
  type Variant = 'primary' | 'secondary' | 'ghost';
  let {
    variant = 'primary',
    href,
    type = 'button',
    children,
    ...rest
  }: {
    variant?: Variant;
    href?: string;
    children?: Snippet;
  } & HTMLButtonAttributes &
    HTMLAnchorAttributes = $props();
</script>

{#if href}
  <a {href} class="wy-btn" data-variant={variant} {...rest}>{@render children?.()}</a>
{:else}
  <button {type} class="wy-btn" data-variant={variant} {...rest}>{@render children?.()}</button>
{/if}

<style>
  .wy-btn {
    display: inline-block;
    font-family: var(--fm);
    font-size: 13px;
    font-weight: 700;
    text-decoration: none;
    padding: 8px 13px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    box-shadow: 3px 3px 0 0 var(--shadow);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
    transition:
      transform 0.04s,
      box-shadow 0.04s;
  }
  .wy-btn:hover {
    transform: translate(-1px, -1px);
    box-shadow: 5px 5px 0 0 var(--shadow);
  }
  .wy-btn:active {
    transform: translate(2px, 2px);
    box-shadow: 1px 1px 0 0 var(--shadow);
  }
  /* the default ring is low contrast on the brand and lime fills */
  .wy-btn:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  .wy-btn:disabled {
    cursor: not-allowed;
    opacity: 0.55;
    transform: none;
    box-shadow: 3px 3px 0 0 var(--shadow);
  }
  /* fill is --brand-btn, never --brand: --brand is a dim navy panel colour in dark mode */
  .wy-btn[data-variant='primary'] {
    background: var(--brand-btn);
    color: var(--brand-btn-ink);
  }
  .wy-btn[data-variant='secondary'] {
    background: var(--lime);
    color: var(--lime-ink);
  }
  .wy-btn[data-variant='ghost'] {
    background: var(--surface);
    color: var(--text);
  }
</style>
