<script lang="ts">
  // Task-first docs navigation, derived from content frontmatter by
  // `navGroups()`. Each first-level section is a labeled list; topic groups
  // inside a section become subheads. The current page carries
  // aria-current="page" (styled in docs.css `.side`).
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
      {#each g.sections as section (section.label)}
        {#if g.sections.length > 1}<h3>{section.label}</h3>{/if}
        <ul>
          {#each section.items as it (it.path)}
            <li>
              <a href={`${base}${it.path}`} aria-current={it.path === current ? 'page' : undefined}
                >{it.label}</a
              >
            </li>
          {/each}
        </ul>
      {/each}
    </section>
  {/each}
</nav>
