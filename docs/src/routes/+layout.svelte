<script lang="ts">
  // The single chrome host (Direction A "Arcade Cabinet"): the always-dark
  // marquee (Mitari wordmark, Wyrd/Fathom switch, search, theme, GitHub), the
  // mobile drawer, the doc shell (sidebar / content + pagination / TOC), and the
  // cabinet footer. The whole app is wrapped in `.mk a` so the global arcade
  // theme (arcade.css → wyrd-tokens.css + fonts) styles everything by class.
  import '../styles/arcade.css';
  import { onMount } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { initLang } from '$lib/lang.svelte';
  import Fuji from '$lib/components/Fuji.svelte';
  import Search from '$lib/components/Search.svelte';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import Toc from '$lib/components/Toc.svelte';
  import Pagination from '$lib/components/Pagination.svelte';

  const GITHUB_URL = 'https://github.com/mitari-ai/wyrd';

  let { children } = $props();

  let theme = $state<'light' | 'dark'>('dark');
  let drawer = $state(false);

  // The home route and error pages render full-width "splash" — no doc shell.
  // The Fathom teaser archetype is also a full-bleed holding page.
  const isHome = $derived(page.route.id === '/');
  const isError = $derived(page.error !== null);
  const isFathom = $derived(page.data?.archetype === 'fathom');
  const isFullWidth = $derived(isHome || isError || isFathom);

  // Active product for the marquee switch.
  const product = $derived(isFathom ? 'fathom' : 'wyrd');

  onMount(() => {
    initLang();
    const t = document.documentElement.dataset.theme;
    theme = t === 'light' ? 'light' : 'dark';
  });

  // close the mobile drawer on navigation
  afterNavigate(() => (drawer = false));

  function toggle(): void {
    theme = theme === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem('wyrd:theme', theme);
    } catch {
      /* ignore */
    }
  }
</script>

<div class="mk a">
  <header class="marquee" data-pagefind-ignore>
    <a class="brand" href={`${base}/`} aria-label="Mitari docs home">
      <Fuji size={22} />
      <span class="wm">MITARI</span>
      <span class="co">docs</span>
    </a>
    <nav class="switch" aria-label="Product">
      <a href={`${base}/`} data-k="wyrd" class={product === 'wyrd' ? 'on' : ''}>Wyrd</a>
      <a href={`${base}/fathom/`} data-k="fathom" class={product === 'fathom' ? 'on' : ''}>
        Fathom <span class="soon">soon</span>
      </a>
    </nav>
    <div class="right">
      <Search />
      <button class="iconbtn theme-toggle" aria-label="Toggle theme" onclick={toggle}>
        {#if theme === 'dark'}
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <circle cx="12" cy="12" r="4.5" />
            <path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M19 5l-1.5 1.5M6.5 17.5 5 19" />
          </svg>
        {:else}
          <svg viewBox="0 0 24 24" fill="currentColor">
            <path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z" />
          </svg>
        {/if}
      </button>
      <a class="iconbtn" href={GITHUB_URL} aria-label="GitHub" rel="noreferrer" target="_blank">
        <svg viewBox="0 0 24 24" fill="currentColor"
          ><path
            d="M12 2a10 10 0 0 0-3.16 19.49c.5.09.68-.22.68-.48v-1.7c-2.78.6-3.37-1.34-3.37-1.34-.45-1.16-1.1-1.47-1.1-1.47-.9-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.9 1.53 2.36 1.09 2.94.83.09-.65.35-1.09.63-1.34-2.22-.25-4.55-1.11-4.55-4.94 0-1.09.39-1.98 1.03-2.68-.1-.25-.45-1.27.1-2.65 0 0 .84-.27 2.75 1.02a9.5 9.5 0 0 1 5 0c1.91-1.29 2.75-1.02 2.75-1.02.55 1.38.2 2.4.1 2.65.64.7 1.03 1.59 1.03 2.68 0 3.84-2.34 4.69-4.57 4.94.36.31.68.92.68 1.85v2.74c0 .27.18.58.69.48A10 10 0 0 0 12 2z"
          /></svg
        >
      </a>
      <button
        class="iconbtn menu nav-drawer"
        aria-label="Menu"
        aria-expanded={drawer}
        aria-controls="nav-drawer"
        onclick={() => (drawer = !drawer)}
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          {#if drawer}<path d="M5 5l14 14M19 5 5 19" />{:else}<path d="M3 6h18M3 12h18M3 18h18" />{/if}
        </svg>
      </button>
    </div>
  </header>

  <nav id="nav-drawer" class={`drawer ${drawer ? 'open' : ''}`} aria-label="Sections">
    <a href={`${base}/overview/`}>Start here</a>
    <a href={`${base}/tutorials/`}>Learn Wyrd</a>
    <a href={`${base}/get-started/`}>Get started</a>
    <a href={`${base}/how-to/`}>Build with Wyrd</a>
    <a href={`${base}/products/`}>Products and components</a>
    <a href={`${base}/reference/`}>Reference</a>
    <a href={`${base}/self-hosting/`}>Operate Wyrd</a>
    <a href={`${base}/for-agents/`}>For agents</a>
    <a href={`${base}/fathom/`}>Fathom · coming soon</a>
  </nav>

  <div id="top">
    {#if isFullWidth}
      {@render children()}
    {:else}
      <div class="shell">
        <aside class="side" data-pagefind-ignore>
          <Sidebar />
        </aside>
        <main class="main">
          {@render children()}
          <Pagination />
        </main>
        <aside data-pagefind-ignore>
          <Toc />
        </aside>
      </div>
    {/if}
  </div>

  <footer class="foot" data-pagefind-ignore>
    <div class="foot-in">
      <div>
        <div class="fb"><Fuji size={22} /><span class="wm">MITARI</span></div>
        <div class="ci">© 2026 Mitari. All rights reserved.<br />Seattle, WA</div>
      </div>
      <div class="fcols">
        <div class="fcol">
          <span class="ch">WYRD</span>
          <a href={`${base}/get-started/`}>Get started</a>
          <a href={`${base}/concepts/`}>Concepts</a>
          <a href={`${base}/products/`}>Products</a>
          <a href={`${base}/reference/`}>Reference</a>
          <a href={`${base}/for-agents/`}>For agents</a>
        </div>
        <div class="fcol">
          <span class="ch">FATHOM</span>
          <a href={`${base}/fathom/`}>Overview</a>
          <a href={`${base}/fathom/`}>Coming soon</a>
        </div>
        <div class="fcol">
          <span class="ch">PROJECT</span>
          <a href={GITHUB_URL} rel="noreferrer" target="_blank">GitHub</a>
          <a href={`${base}/overview/`}>Overview</a>
          <a href={`${base}/llms.txt`}>llms.txt</a>
        </div>
      </div>
    </div>
  </footer>
</div>
