<script lang="ts">
  import { tick } from 'svelte';
  import { page } from '$app/state';

  // Right-rail table of contents, parsed from the rendered article headings
  // (rehype-slug already put ids on every h2/h3; autolink wrapped the text).
  // Re-parses on navigation and runs an IntersectionObserver scroll-spy.
  type Head = { id: string; text: string; level: number };

  let heads = $state<Head[]>([]);
  let activeId = $state<string>('');

  $effect(() => {
    // depend on the path so the TOC rebuilds on client-side navigation
    void page.url.pathname;
    let observer: IntersectionObserver | undefined;

    (async () => {
      await tick();
      const article = document.querySelector('.sl-markdown-content');
      if (!article) {
        heads = [];
        return;
      }
      const nodes = Array.from(
        article.querySelectorAll<HTMLElement>('h2[id], h3[id]')
      );
      heads = nodes.map((n) => ({
        id: n.id,
        text: n.textContent?.trim() ?? '',
        level: n.tagName === 'H2' ? 2 : 3
      }));
      activeId = heads[0]?.id ?? '';

      observer = new IntersectionObserver(
        (entries) => {
          for (const e of entries) {
            if (e.isIntersecting) activeId = (e.target as HTMLElement).id;
          }
        },
        { rootMargin: '0px 0px -75% 0px', threshold: 0 }
      );
      for (const n of nodes) observer.observe(n);
    })();

    return () => observer?.disconnect();
  });
</script>

{#if heads.length > 1}
  <nav class="wy-toc" aria-label="On this page">
    <div class="toc-title">On this page</div>
    <ul>
      {#each heads as h (h.id)}
        <li class:sub={h.level === 3}>
          <a href={`#${h.id}`} aria-current={activeId === h.id ? 'true' : undefined}>{h.text}</a>
        </li>
      {/each}
    </ul>
  </nav>
{/if}

<style>
  .wy-toc {
    position: sticky;
    top: 84px;
    font-family: var(--font-mono);
  }
  .toc-title {
    font-size: 0.62rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 10px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border-left: 2px solid var(--border);
  }
  li {
    margin: 0;
  }
  li.sub a {
    padding-left: 22px;
    font-size: 0.7rem;
  }
  a {
    display: block;
    padding: 4px 12px;
    font-size: 0.74rem;
    color: var(--muted);
    text-decoration: none;
    margin-left: -2px;
    border-left: 2px solid transparent;
  }
  a:hover {
    color: var(--text);
  }
  a[aria-current='true'] {
    color: var(--rune-strong);
    font-weight: 700;
    border-left-color: var(--rune-strong);
  }
</style>
