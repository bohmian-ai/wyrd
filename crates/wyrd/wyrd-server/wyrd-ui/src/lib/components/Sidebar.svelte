<script lang="ts">
  type Kind = 'client' | 'server' | 'control';
  type NavItem = { label: string; href?: string; active?: boolean; kind?: Kind };
  type NavGroup = { label?: string; items: NavItem[] };

  let { brand, groups }: { brand?: string; groups: NavGroup[] } = $props();
</script>

<nav class="wy-sidebar">
  {#if brand}<div class="brand"><i></i>{brand}</div>{/if}
  {#each groups as g (g.label ?? g.items[0]?.label)}
    {#if g.label}<div class="grp">{g.label}</div>{/if}
    {#each g.items as it (it.label)}
      <a class="item" class:active={it.active} data-k={it.kind} href={it.href ?? '#'}>{it.label}</a>
    {/each}
  {/each}
</nav>

<style>
  .wy-sidebar {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 11px;
    color: var(--text);
  }
  .brand {
    font-family: var(--fh);
    font-size: 15px;
    letter-spacing: -0.3px;
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 10px;
    padding: 0 5px;
  }
  .brand i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--lime);
    flex: 0 0 auto;
  }
  .grp {
    font-family: var(--fm);
    font-size: 8.5px;
    font-weight: 700;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--muted);
    margin: 12px 5px 5px;
  }
  .item {
    font-family: var(--fm);
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    text-decoration: none;
    padding: 7px 9px;
    border: 2px solid transparent;
    border-radius: var(--r);
    display: block;
  }
  .item:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .item.active {
    color: var(--text);
    background: var(--brand-soft);
    border-color: var(--border);
    box-shadow: 2px 2px 0 0 var(--shadow);
  }
  .item[data-k='client'] {
    border-left: 4px solid var(--client-bar);
  }
  .item[data-k='server'] {
    border-left: 4px solid var(--server-bar);
  }
  .item[data-k='control'] {
    border-left: 4px solid var(--control-bar);
  }
</style>
