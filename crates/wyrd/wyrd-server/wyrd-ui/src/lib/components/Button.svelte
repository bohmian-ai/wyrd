<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  type Variant = 'primary' | 'rune' | 'ghost';
  let {
    variant = 'primary',
    type = 'button',
    children,
    ...rest
  }: { variant?: Variant; children?: Snippet } & HTMLButtonAttributes = $props();
</script>

<button {type} class="wy-btn" data-variant={variant} {...rest}>
  {@render children?.()}
</button>

<style>
  .wy-btn {
    font-family: var(--fm);
    font-size: 13px;
    font-weight: 700;
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
  .wy-btn[data-variant='primary'] {
    background: var(--lime);
    color: var(--lime-ink);
  }
  .wy-btn[data-variant='rune'] {
    background: var(--rune-btn);
    color: var(--rune-btn-ink);
  }
  .wy-btn[data-variant='ghost'] {
    background: var(--surface);
    color: var(--text);
  }
</style>
