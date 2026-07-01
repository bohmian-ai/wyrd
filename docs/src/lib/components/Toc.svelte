<script lang="ts">
  // Direction A "On this page" rail (`.toc`): a `.th` label over `<a>` links, h3s
  // marked `.sub` and the in-view heading `.on`. Markup + classes only — styled by
  // the global arcade theme (arcade.css `.mk .toc …`). Headings are still parsed
  // live from the rendered article (`.prose`) and tracked with an
  // IntersectionObserver scrollspy.
  import { tick } from 'svelte';
  import { page } from '$app/state';

  type Head = { id: string; text: string; level: number };

  let heads = $state<Head[]>([]);
  let activeId = $state<string>('');

  $effect(() => {
    void page.url.pathname;
    let observer: IntersectionObserver | undefined;

    (async () => {
      await tick();
      const article = document.querySelector('.prose');
      if (!article) {
        heads = [];
        return;
      }
      const nodes = Array.from(article.querySelectorAll<HTMLElement>('h2[id], h3[id]'));
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
  <nav class="toc" aria-label="On this page">
    <div class="th">On this page</div>
    {#each heads as h (h.id)}
      <a
        href={`#${h.id}`}
        class={`${h.level === 3 ? 'sub' : ''} ${activeId === h.id ? 'on' : ''}`}
        aria-current={activeId === h.id ? 'true' : undefined}>{h.text}</a
      >
    {/each}
  </nav>
{/if}
