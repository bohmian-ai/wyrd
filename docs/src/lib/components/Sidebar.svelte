<script lang="ts">
  // Direction A doc sidebar: each nav group is a collapsible
  // `<details class="grp">` whose `<summary class="gh">` carries the colored
  // `.b` kind bar + label, followed by `<a>` item links (active one marked
  // `.on`). Native `<details>` keeps this zero-runtime and keyboard-accessible;
  // the group holding the current page defaults open, the rest start collapsed.
  // Markup + classes only — every rule lives in the global arcade theme
  // (arcade.css `.mk .side …`). Nav is derived from the content tree via
  // `navGroups()`.
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { navGroups, stripBase } from '$lib/derived-nav';

  const nav = navGroups();

  function norm(p: string): string {
    return p === '/' ? '/' : p.replace(/\/$/, '');
  }

  const current = $derived(norm(stripBase(page.url.pathname, base || '/wyrd')));

  function isActive(path: string): boolean {
    return norm(path) === current;
  }

  function hasActive(items: { path: string }[]): boolean {
    return items.some((it) => isActive(it.path));
  }
</script>

<nav aria-label="Documentation">
  {#each nav as g (g.label)}
    <details class="grp" open={hasActive(g.items)}>
      <summary class="gh"><span class={`b ${g.kind}`}></span>{g.label}</summary>
      {#each g.items as it (it.path)}
        <a
          href={`${base}${it.path}`}
          class={isActive(it.path) ? 'on' : ''}
          aria-current={isActive(it.path) ? 'page' : undefined}
        >
          {it.label}
        </a>
      {/each}
    </details>
  {/each}
  <details class="grp">
    <summary class="gh"><span class="b rune"></span>Fathom</summary>
    <a href={`${base}/fathom/`}>Overview <span class="soon">soon</span></a>
  </details>
</nav>
