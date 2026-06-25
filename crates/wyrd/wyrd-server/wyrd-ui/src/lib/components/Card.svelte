<script lang="ts">
  import type { Snippet } from 'svelte';

  type Variant = 'quiet' | 'raised' | 'wide';
  let {
    variant = 'quiet',
    head,
    children
  }: { variant?: Variant; head?: Snippet; children?: Snippet } = $props();
</script>

<section class="wy-card" data-variant={variant}>
  {#if head}
    <header class="wy-card-head">{@render head()}</header>
  {/if}
  <div class="wy-card-body">{@render children?.()}</div>
</section>

<style>
  .wy-card {
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    box-shadow: 3px 3px 0 0 var(--shadow);
    overflow: hidden;
  }
  .wy-card[data-variant='raised'] {
    box-shadow: 6px 6px 0 0 var(--shadow);
  }
  .wy-card[data-variant='wide'] {
    grid-column: 1 / -1;
  }
  .wy-card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 11px;
    border-bottom: 2px solid var(--border);
    font-family: var(--fm);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--muted);
  }
  .wy-card-body {
    padding: 12px;
  }
</style>
