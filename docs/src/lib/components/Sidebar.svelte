<script lang="ts">
  // Docs navigation, derived from content frontmatter by `navGroups()`: each
  // published section is a collapsible list, and Build and verify's topics are
  // collapsible subgroups. Only the section and topic holding the current page
  // start open (Get started when no page matches, as on the landing); the rest
  // open on click. The current page carries aria-current="page" (styled in
  // docs.css `.side`).
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { locate, navGroups } from '$lib/derived-nav';

  const nav = navGroups();
  const here = $derived(locate(page.url.pathname));
  const current = $derived(here?.item.path);
  const openSection = $derived(here?.section ?? nav[0]?.label);
</script>

<nav aria-label="Documentation">
  {#each nav as g (g.label)}
    <details open={g.label === openSection}>
      <summary><h2>{g.label}</h2></summary>
      {#each g.topics as topic (topic.label)}
        {#if topic.label}
          <details class="topic" open={topic.items.some((it) => it.path === current)}>
            <summary><h3>{topic.label}</h3></summary>
            {@render items(topic.items)}
          </details>
        {:else}
          {@render items(topic.items)}
        {/if}
      {/each}
    </details>
  {/each}
</nav>

{#snippet items(list: { label: string; path: string }[])}
  <ul>
    {#each list as it (it.path)}
      <li>
        <a href={`${base}${it.path}`} aria-current={it.path === current ? 'page' : undefined}>{it.label}</a>
      </li>
    {/each}
  </ul>
{/snippet}
