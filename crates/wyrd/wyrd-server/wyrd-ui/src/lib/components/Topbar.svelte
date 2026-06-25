<script lang="ts">
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
    {#if env}<span class="env"><i data-st={env.status}></i>{env.label}</span>{/if}
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
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
    min-width: 0;
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
  }
  .search {
    font-family: var(--fm);
    font-size: 11px;
    color: var(--text);
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 5px 9px;
    background: var(--surface-2);
    min-width: 180px;
  }
  .search::placeholder {
    color: var(--muted);
  }
  .env {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--text);
    border: 2px solid var(--border);
    border-radius: var(--r);
    padding: 4px 9px;
    background: var(--surface);
    white-space: nowrap;
  }
  .env i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
  .env i[data-st='ok'] {
    background: var(--ok);
  }
  .env i[data-st='warn'] {
    background: var(--warn);
  }
  .env i[data-st='danger'] {
    background: var(--danger);
  }
</style>
