<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { SessionMetadata, Tenant } from '$lib/views';
  import { toggleMode, theme } from '$lib/theme.svelte';
  import logo from '../../../../brand/logo.svg?inline';
  import TenantChooser from './TenantChooser.svelte';

  let {
    tenant,
    session,
    pathname,
    navCollapsed = false,
    children
  }: {
    tenant: Tenant;
    session: SessionMetadata;
    pathname: string;
    navCollapsed?: boolean;
    children: Snippet;
  } = $props();
  let navOpen = $state(false);
  // Sidebar collapse is a viewer preference persisted in a cookie so SSR renders
  // the chosen width on first byte — same mechanism as the theme mode. The prop
  // is deliberately only the initial value: after hydration the toggle owns it.
  // svelte-ignore state_referenced_locally
  let collapsed = $state(navCollapsed);
  function toggleCollapsed() {
    collapsed = !collapsed;
    document.cookie = `wyrd-nav=${collapsed ? 'collapsed' : 'open'}; path=/; max-age=31536000; samesite=lax`;
  }
  let menuButton: HTMLButtonElement;
  function closeMenu() {
    navOpen = false;
    menuButton?.focus();
  }
  const entries = [
    ['Home', ''],
    ['Cards', '/cards'],
    ['Observe', '/observe'],
    ['Changes', '/changes'],
    ['Query', '/query']
  ];
  let base = $derived(`/t/${encodeURIComponent(tenant.key)}`);
  let area = $derived(
    entries.find(
      ([, suffix]) =>
        suffix && (pathname === base + suffix || pathname.startsWith(base + suffix + '/'))
    )
  );
  let current = $derived(area?.[0] ?? 'Home');
  // Deep-route segments after the area root, so drilldown pages keep their
  // place in the topbar (e.g. acme / Changes / change_01 / verification).
  let trail = $derived(
    area
      ? pathname
          .slice((base + area[1]).length)
          .split('/')
          .filter(Boolean)
      : []
  );
</script>

<svelte:window onkeydown={(event) => { if (event.key === 'Escape' && navOpen) closeMenu(); }} />
<a class="skip" href="#main">Skip to content</a>
<div class="app-shell" class:rail={collapsed}>
  <aside>
    <a class="wordmark" href={base}><img src={logo} alt="" width="28" height="28" />{#if !collapsed}bohmian{/if}</a>
    <button class="app-control mobile-menu" type="button" bind:this={menuButton} aria-controls="menu-disclosure" aria-expanded={navOpen} onclick={() => navOpen = !navOpen}>Menu</button>
    <div id="menu-disclosure" class="menu-panel" class:collapsed={!navOpen} role="region" aria-label="Menu">
      <div class="menu-context mobile-menu"><span>Current area: {current}</span><span>{tenant.name}</span><button class="app-control" type="button" onclick={closeMenu}>Close menu</button></div>
    <nav id="primary-navigation" aria-label="Primary">
      {#each entries as [label, suffix] (label)}
        <a href={base + suffix} aria-current={current === label ? 'page' : undefined} title={collapsed ? label : undefined}><span class="nav-full">{label}</span><span class="nav-mini" aria-hidden="true">{label.slice(0, 2)}</span></a>
      {/each}
    </nav>
    </div>
    <button class="collapse-toggle" type="button" onclick={toggleCollapsed} aria-expanded={!collapsed} aria-label={collapsed ? 'Expand navigation' : 'Collapse navigation'}>{collapsed ? '»' : '« Collapse'}</button>
  </aside>
  <div class="workspace">
    <header>
      <span class="product">WYRD</span><span class="context">{tenant.key} / <strong>{current}</strong>{#each trail as segment, i (i)}{' / '}{decodeURIComponent(segment)}{/each}</span>
      <div class="identity">
        {#if session.tenants.length > 1}
          <details class="tenant-menu">
            <summary aria-label="Current tenant">{tenant.name}</summary>
            <TenantChooser {session} />
          </details>
        {:else}
          <span aria-label="Current tenant">{tenant.name}</span>
        {/if}
        <details class="principal">
          <summary>{session.subject.name}</summary>
          <form method="POST" action="/?/logout">
            <input type="hidden" name="csrf" value={session.csrf} />
            <button class="app-control" type="submit">Sign out</button>
          </form>
        </details>
        <button class="app-control" type="button" onclick={toggleMode} aria-label={`Switch to ${theme.mode === 'dark' ? 'light' : 'dark'} mode`}>◐ {theme.mode === 'dark' ? 'Light' : 'Dark'}</button>
      </div>
    </header>
    <main id="main" tabindex="-1">{@render children()}</main>
  </div>
</div>

<style>
  .app-shell {
    min-height: 100dvh;
    display: grid;
    grid-template-columns: 212px minmax(0, 1fr);
  }
  .app-shell.rail {
    grid-template-columns: 60px minmax(0, 1fr);
  }
  aside {
    background: var(--surface);
    border-right: 2px solid var(--border);
    padding: 16px 14px;
    display: flex;
    flex-direction: column;
  }
  .rail aside {
    padding: 16px 10px;
    align-items: center;
  }
  .rail nav a {
    padding: 7px 0;
    text-align: center;
  }
  .nav-mini {
    display: none;
  }
  @media (min-width: 768px) {
    .rail .nav-full {
      display: none;
    }
    .rail .nav-mini {
      display: inline;
    }
  }
  .collapse-toggle {
    margin-top: auto;
    padding: 5px 8px;
    border: 2px solid transparent;
    border-radius: var(--r);
    background: none;
    color: var(--muted);
    font: 700 10px var(--font-mono);
    cursor: pointer;
    text-align: left;
  }
  .collapse-toggle:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .wordmark {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text);
    text-decoration: none;
    font: 600 18px var(--font-serif);
    margin-bottom: 34px;
  }
  nav {
    display: grid;
    gap: 6px;
    font: 700 11px var(--font-mono);
  }
  nav a {
    padding: 7px 12px;
    color: var(--muted);
    text-decoration: none;
    border: 2px solid transparent;
    border-radius: var(--r);
  }
  nav a:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  nav a[aria-current] {
    color: var(--text);
    border-color: var(--border);
    background: var(--brand-soft);
    box-shadow: 3px 3px 0 var(--shadow);
  }
  header {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 14px;
    min-height: 56px;
    padding: 10px 22px;
    border-bottom: 2px solid var(--border);
    background: var(--surface);
    font: 700 11px var(--font-mono);
  }
  .product {
    background: var(--brand-btn);
    color: var(--brand-btn-ink);
    padding: 5px 9px;
    border: 2px solid var(--border);
    border-radius: var(--r);
  }
  .wordmark img {
    flex: 0 0 28px;
    border-radius: var(--r);
  }
  .workspace {
    min-width: 0;
  }
  header :global(.app-control),
  header summary {
    padding: 4px 8px;
    border: 2px solid var(--border);
    border-radius: var(--r);
    background: var(--surface-2);
    font: 700 10px var(--font-mono);
    box-shadow: none;
  }
  .context strong {
    color: var(--text);
  }
  .context {
    color: var(--muted);
  }
  .identity {
    margin-left: auto;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }
  summary {
    cursor: pointer;
  }
  .principal form {
    margin-top: 12px;
  }
  main {
    padding: 24px;
    min-width: 0;
  }
  .menu-panel {
    display: contents;
  }
  .mobile-menu {
    display: none;
  }
  .skip {
    position: absolute;
    z-index: 2;
    top: -80px;
    left: 12px;
    padding: 7px 12px;
    background: var(--surface);
    color: var(--text);
  }
  .skip:focus {
    top: 12px;
  }
  @media (max-width: 767px) {
    .app-shell,
    .app-shell.rail {
      grid-template-columns: minmax(0, 1fr);
    }
    .collapse-toggle {
      display: none;
    }
    aside {
      border-right: 0;
      border-bottom: 2px solid var(--border);
      padding: 14px 18px;
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: 12px;
    }
    .wordmark {
      margin-bottom: 0;
    }
    .mobile-menu {
      display: block;
    }
    nav {
      margin-top: 16px;
    }
    .menu-panel {
      display: block;
      width: 100%;
      padding: 14px;
      border: 2px solid var(--border);
      border-radius: var(--r);
      box-shadow: 3px 3px 0 var(--shadow);
    }
    .menu-context {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: 10px;
      font: 11px var(--font-mono);
    }
    .menu-context button {
      margin-left: auto;
    }
    aside > button {
      order: -1;
    }
    .menu-panel.collapsed {
      display: none;
    }
    header {
      padding: 14px 18px;
    }
    .identity {
      margin-left: 0;
      width: 100%;
    }
    main {
      padding: 22px 18px;
    }
  }
</style>
