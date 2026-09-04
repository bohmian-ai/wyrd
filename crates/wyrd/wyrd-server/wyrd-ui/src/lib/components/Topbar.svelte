<script lang="ts">
  import Badge from './Badge.svelte';

  type Env = { label: string; status: 'ok' | 'warn' | 'danger' };

  let {
    crumbs,
    search,
    env
  }: { crumbs: string[]; search?: string; env?: Env } = $props();
</script>

<div class="wy-topbar">
  <nav class="crumbs">
    {#each crumbs as c, i (c)}
      {#if i > 0}<span class="sep">/</span>{/if}
      <span class="crumb" class:cur={i === crumbs.length - 1}>{c}</span>
    {/each}
  </nav>
  <div class="right">
    {#if search}<input class="search" type="search" placeholder={search} />{/if}
    <!-- the environment reads through Badge so its state carries a glyph and the tone is
         never the only signal; a bare coloured dot fails INV-008 -->
    {#if env}<Badge tone={env.status}>{env.label}</Badge>{/if}
  </div>
</div>

<style>
  .wy-topbar {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 10px 14px;
    font-family: var(--fm);
    color: var(--text);
    flex-wrap: wrap;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
    min-width: 0;
    flex-shrink: 1;
    overflow: hidden;
  }
  .crumb {
    color: var(--muted);
  }
  .crumb.cur {
    color: var(--text);
    font-weight: 700;
  }
  .sep {
    color: var(--muted);
  }
  .right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    flex: 1 1 auto;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  .search {
    font-family: var(--fm);
    font-size: 11px;
    color: var(--text);
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 5px 9px;
    background: var(--surface-2);
    flex: 1 1 140px;
    min-width: 0;
  }
  .search::placeholder {
    color: var(--muted);
  }
</style>
