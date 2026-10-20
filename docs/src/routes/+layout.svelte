<script lang="ts">
  // Docs chrome: Wyrd header (mark, search, reference, theme, GitHub), the
  // task-first sidebar (a drawer on narrow screens), the article column with
  // its current-location cue and pager, and the on-page contents rail. Styled
  // by styles/docs.css over the generated Evidence Thread tokens. Search and
  // internal shell links appear only once published pages exist to reach.
  import '../styles/docs.css';
  import { onMount } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { initLang } from '$lib/lang.svelte';
  import { firstPage, locate, navGroups } from '$lib/derived-nav';
  import { setTheme, type Theme } from '$lib/theme';
  import WyrdMark from '$lib/components/WyrdMark.svelte';
  import Search from '$lib/components/Search.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toc from '$lib/components/Toc.svelte';
  import Pagination from '$lib/components/Pagination.svelte';

  const GITHUB_URL = 'https://github.com/bohmian-ai/wyrd';
  // Footer horizon grid: rows tighten toward the horizon; rays converge on the sun.
  const FLOOR_ROWS = [43, 47, 53];
  const FLOOR_RAYS = Array.from({ length: 16 }, (_, i) => 1120 + (i - 12) * 110);

  let { children } = $props();

  let theme = $state<Theme>('light');
  let drawer = $state(false);
  let drawerToggle = $state<HTMLButtonElement>();

  // Home and error pages render without the doc shell.
  const isFullWidth = $derived(page.route.id === '/' || page.error !== null);
  const here = $derived(locate(page.url.pathname, base || '/wyrd'));
  const hasPages = navGroups().length > 0;
  const start = firstPage('Get started');
  const reference = firstPage('Reference');

  onMount(() => {
    initLang();
    theme = document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
  });

  afterNavigate(() => (drawer = false));

  // Escape closes the open drawer and returns focus to its toggle, so keyboard
  // users are never left on a link that just became hidden.
  function closeDrawerOnEscape(event: KeyboardEvent): void {
    if (event.key !== 'Escape' || !drawer) return;
    drawer = false;
    drawerToggle?.focus();
  }

  function toggleTheme(): void {
    theme = theme === 'dark' ? 'light' : 'dark';
    setTheme(theme);
  }
</script>

<svelte:window onkeydown={closeDrawerOnEscape} />

<a class="skip" href="#content">Skip to content</a>

<header class="site-head" data-pagefind-ignore>
  <a class="lockup" href={`${base}/`} aria-label="Wyrd docs home">
    <WyrdMark size={28} label="Wyrd" />
    <span class="word">Wyrd</span>
    <span class="tag">docs</span>
    <span class="tag">v{__WYRD_VERSION__}</span>
  </a>
  {#if hasPages}<Search />{/if}
  {#if reference}<a class="head-link" href={`${base}${reference.path}`}>Reference</a>{/if}
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
      bind:this={drawerToggle}
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
  <!-- The 404 already shows the full cover scene above the footer. -->
  {#if !page.error}
  <svg class="foot-horizon" viewBox="0 0 1440 56" preserveAspectRatio="xMaxYMax slice" aria-hidden="true">
    <path class="sun" d="M1078 40a42 42 0 0 1 84 0z" />
    <rect class="band" x="1076" y="22" width="88" height="2" />
    <rect class="band" x="1076" y="29" width="88" height="3" />
    <rect class="band" x="1076" y="35" width="88" height="4" />
    {#each FLOOR_ROWS as y (y)}<line class="grid" x1="0" y1={y} x2="1440" y2={y} />{/each}
    {#each FLOOR_RAYS as x (x)}<line class="grid" x1={1120 + (x - 1120) * 0.04} y1="40" x2={x} y2="56" />{/each}
    <line class="horizon" x1="0" y1="40" x2="1440" y2="40" />
  </svg>
  {/if}
  <nav aria-label="Footer">
    {#if start}<a href={`${base}${start.path}`}>Get started</a>{/if}
    {#if reference}<a href={`${base}${reference.path}`}>Reference</a>{/if}
    <a href={`${base}/llms.txt`}>llms.txt</a>
    <a href={GITHUB_URL} rel="noreferrer" target="_blank">GitHub</a>
  </nav>
</footer>
