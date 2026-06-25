<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    sidebar,
    topbar,
    children
  }: { sidebar?: Snippet; topbar?: Snippet; children?: Snippet } = $props();
</script>

<div class="wy-shell">
  {#if sidebar}<aside class="shell-side">{@render sidebar()}</aside>{/if}
  <div class="shell-main">
    {#if topbar}<header class="shell-top">{@render topbar()}</header>{/if}
    <div class="shell-content">{@render children?.()}</div>
  </div>
</div>

<style>
  .wy-shell {
    display: flex;
    border: 3px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
    overflow: hidden;
    color: var(--text);
  }
  .shell-side {
    flex: 0 0 212px;
    border-right: 2px solid var(--border);
    background: var(--surface);
  }
  .shell-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .shell-top {
    border-bottom: 2px solid var(--border);
    background: var(--surface);
  }
  .shell-content {
    flex: 1;
    min-width: 0;
    padding: 16px;
  }
</style>
