<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { nav, stripBase, type NavGroup } from '$lib/derived-nav';

  // The doc-shell sidebar: nav.ts groups with the "Line" group rule (a colored
  // bar per group kind), collapsible groups, and active-state from the router.
  // Top-level standalone links (single item whose label matches its only item)
  // render as a bare link, not a collapsible group.

  function norm(p: string): string {
    return p === '/' ? '/' : p.replace(/\/$/, '');
  }

  const current = $derived(norm(stripBase(page.url.pathname, base || '/wyrd')));

  function isActive(path: string): boolean {
    return norm(path) === current;
  }

  function isStandalone(g: NavGroup): boolean {
    return g.items.length === 1 && g.items[0].label === g.label;
  }

  function groupHasActive(g: NavGroup): boolean {
    return g.items.some((it) => isActive(it.path));
  }
</script>

<nav class="wy-doc-side" aria-label="Documentation">
  {#each nav as g (g.label)}
    {#if isStandalone(g)}
      <a
        class="solo"
        data-k={g.kind}
        href={`${base}${g.items[0].path}`}
        aria-current={isActive(g.items[0].path) ? 'page' : undefined}
        class:active={isActive(g.items[0].path)}
      >
        {g.items[0].label}
      </a>
    {:else}
      <details open={groupHasActive(g) || true}>
        <summary data-k={g.kind}><span>{g.label}</span></summary>
        <div class="grp-items">
          {#each g.items as it (it.path)}
            <a
              class="item"
              href={`${base}${it.path}`}
              aria-current={isActive(it.path) ? 'page' : undefined}
              class:active={isActive(it.path)}
            >
              {it.label}
            </a>
          {/each}
        </div>
      </details>
    {/if}
  {/each}
</nav>

<style>
  .wy-doc-side {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 16px 12px 32px;
    color: var(--text);
  }
  details {
    margin: 0;
  }
  summary {
    list-style: none;
    cursor: pointer;
    font-family: var(--font-mono);
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    margin: 12px 0 5px;
    padding: 3px 8px;
    /* the Line group rule — a colored left bar per group kind */
    border-left: 3px solid var(--rune-strong);
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary[data-k='client'] {
    border-left-color: var(--client-bar);
  }
  summary[data-k='server'] {
    border-left-color: var(--server-bar);
  }
  summary[data-k='control'] {
    border-left-color: var(--control-bar);
  }
  summary[data-k='rune'] {
    border-left-color: var(--rune-strong);
  }
  .grp-items {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .item,
  .solo {
    font-family: var(--font-mono);
    font-size: 0.78rem;
    font-weight: 600;
    color: var(--muted);
    text-decoration: none;
    padding: 6px 9px;
    border: 2px solid transparent;
    border-radius: var(--r);
    display: block;
  }
  .solo {
    margin-top: 6px;
    border-left: 3px solid var(--rune-strong);
  }
  .solo[data-k='client'] {
    border-left-color: var(--client-bar);
  }
  .solo[data-k='server'] {
    border-left-color: var(--server-bar);
  }
  .solo[data-k='control'] {
    border-left-color: var(--control-bar);
  }
  .solo[data-k='rune'] {
    border-left-color: var(--rune-strong);
  }
  .item:hover,
  .solo:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .item.active,
  .solo.active {
    color: var(--text);
    background: var(--rune-soft);
    border-color: var(--border);
    box-shadow: 2px 2px 0 0 var(--shadow);
  }
  /* Phone drawer: roomier rows for touch (the desktop rail stays dense). */
  @media (max-width: 820px) {
    .item,
    .solo {
      padding: 10px 11px;
      font-size: 0.82rem;
    }
    summary {
      padding: 8px 8px;
    }
  }
</style>
