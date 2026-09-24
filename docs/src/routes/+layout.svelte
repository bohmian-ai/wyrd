<script lang="ts">
  // Docs chrome: Wyrd header (mark, search, reference, theme, GitHub), the
  // task-first sidebar (a drawer on narrow screens), the article column with
  // its current-location cue and pager, and the on-page contents rail. Styled
  // by styles/docs.css over the generated Evidence Thread tokens.
  import '../styles/docs.css';
  import { onMount } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { initLang } from '$lib/lang.svelte';
  import { locate } from '$lib/derived-nav';
  import { setTheme, type Theme } from '$lib/theme';
  import WyrdMark from '$lib/components/WyrdMark.svelte';
  import Search from '$lib/components/Search.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toc from '$lib/components/Toc.svelte';
  import Pagination from '$lib/components/Pagination.svelte';

  const GITHUB_URL = 'https://github.com/bohmian-ai/wyrd';

  let { children } = $props();

  let theme = $state<Theme>('light');
  let drawer = $state(false);

  // Home and error pages render without the doc shell.
  const isFullWidth = $derived(page.route.id === '/' || page.error !== null);
  const here = $derived(locate(page.url.pathname, base || '/wyrd'));

  onMount(() => {
    initLang();
    theme = document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
  });

  afterNavigate(() => (drawer = false));

  function toggleTheme(): void {
    theme = theme === 'dark' ? 'light' : 'dark';
    setTheme(theme);
  }
</script>

<a class="skip" href="#content">Skip to content</a>

<header class="site-head" data-pagefind-ignore>
  <a class="lockup" href={`${base}/`} aria-label="Wyrd docs home">
    <WyrdMark size={28} label="Wyrd" />
    <span class="word">Wyrd</span>
    <span class="tag">docs</span>
  </a>
  <Search />
  <a class="head-link" href={`${base}/reference/`}>Reference</a>
  <a class="head-link" href={GITHUB_URL} rel="noreferrer" target="_blank">GitHub</a>
  <button
    class="icon-control theme-toggle"
    type="button"
    aria-label={theme === 'dark' ? 'Use light theme' : 'Use dark theme'}
    onclick={toggleTheme}
  >
    <svg class="sun" viewBox="0 0 20 20" aria-hidden="true">
      <circle cx="10" cy="10" r="3.5" fill="none" stroke="currentColor" stroke-width="1.5" />
      <path
        d="M10 2v2M10 16v2M2 10h2M16 10h2M4.3 4.3l1.4 1.4M14.3 14.3l1.4 1.4M15.7 4.3l-1.4 1.4M5.7 14.3l-1.4 1.4"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
      />
    </svg>
    <svg class="moon" viewBox="0 0 20 20" aria-hidden="true">
      <path d="M15.8 12.8A6.4 6.4 0 0 1 7.2 4.2a6.4 6.4 0 1 0 8.6 8.6Z" fill="none" stroke="currentColor" stroke-width="1.5" />
    </svg>
    <span class="label">{theme === 'dark' ? 'Dark' : 'Light'}</span>
  </button>
  {#if !isFullWidth}
    <button
      class="icon-control nav-drawer"
      type="button"
      aria-label={drawer ? 'Close navigation' : 'Open navigation'}
      aria-expanded={drawer}
      aria-controls="docs-nav"
      onclick={() => (drawer = !drawer)}
    >
      <svg viewBox="0 0 20 20" aria-hidden="true">
        {#if drawer}
          <path d="M5 5l10 10M15 5 5 15" fill="none" stroke="currentColor" stroke-width="1.5" />
        {:else}
          <path d="M3 5h14M3 10h14M3 15h14" fill="none" stroke="currentColor" stroke-width="1.5" />
        {/if}
      </svg>
    </button>
  {/if}
</header>

{#if isFullWidth}
  <main id="content">
    {@render children()}
  </main>
{:else}
  {#if drawer}
    <button class="scrim" type="button" tabindex="-1" aria-label="Close navigation" onclick={() => (drawer = false)}
    ></button>
  {/if}
  <div class="shell">
    <aside id="docs-nav" class="side" class:open={drawer} data-pagefind-ignore>
      <Sidebar />
    </aside>
    <main id="content" class="main">
      {#if here}
        <p class="crumbs" data-pagefind-ignore>{here.section} / {here.item.label}</p>
      {/if}
      {@render children()}
      <Pagination />
    </main>
    <aside class="toc-rail" data-pagefind-ignore>
      <Toc />
    </aside>
  </div>
{/if}

<footer class="site-foot" data-pagefind-ignore>
  <nav aria-label="Footer">
    <a href={`${base}/get-started/`}>Run Wyrd locally</a>
    <a href={`${base}/reference/`}>Reference</a>
    <a href={`${base}/llms.txt`}>llms.txt</a>
    <a href={GITHUB_URL} rel="noreferrer" target="_blank">GitHub</a>
  </nav>
</footer>
