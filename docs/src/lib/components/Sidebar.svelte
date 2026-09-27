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
        <!-- A topic's index page shares its label, so the subhead links to it instead of repeating it. -->
        {@const index = g.sections.length > 1 && section.items[0]?.label === section.label ? section.items[0] : undefined}
        {#if g.sections.length > 1}
          <h3>
            {#if index}<a href={`${base}${index.path}`} aria-current={index.path === current ? 'page' : undefined}
                >{section.label}</a
              >{:else}{section.label}{/if}
          </h3>
        {/if}
        {#if section.items.length > (index ? 1 : 0)}
          <ul>
            {#each index ? section.items.slice(1) : section.items as it (it.path)}
              <li>
                <a href={`${base}${it.path}`} aria-current={it.path === current ? 'page' : undefined}
                  >{it.label}</a
                >
              </li>
            {/each}
          </ul>
        {/if}
      {/each}
    </section>
  {/each}
</nav>
