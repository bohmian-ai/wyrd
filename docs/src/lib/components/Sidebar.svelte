<script lang="ts">
  // Docs navigation, derived from content frontmatter by `navGroups()`: each
  // published section is a labeled list, and Build and verify's topics become
  // subheads. The current page carries aria-current="page" (styled in docs.css
  // `.side`).
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { locate, navGroups } from '$lib/derived-nav';

  const nav = navGroups();
  const current = $derived(locate(page.url.pathname, base || '/wyrd')?.item.path);
</script>

<nav aria-label="Documentation">
  {#each nav as g (g.label)}
    <section>
      <h2>{g.label}</h2>
      {#each g.topics as topic (topic.label)}
        {#if topic.label}<h3>{topic.label}</h3>{/if}
        <ul>
          {#each topic.items as it (it.path)}
            <li>
              <a href={`${base}${it.path}`} aria-current={it.path === current ? 'page' : undefined}>{it.label}</a>
            </li>
          {/each}
        </ul>
      {/each}
    </section>
  {/each}
</nav>
