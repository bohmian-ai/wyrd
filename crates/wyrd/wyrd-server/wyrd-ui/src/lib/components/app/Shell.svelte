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
    children
  }: {
    tenant: Tenant;
    session: SessionMetadata;
    pathname: string;
    children: Snippet;
  } = $props();
  let navOpen = $state(true);
  const entries = [
    ['Home', ''],
    ['Cards', '/cards'],
    ['Observe', '/observe'],
    ['Changes', '/changes'],
    ['Query', '/query']
  ];
  let base = $derived(`/t/${encodeURIComponent(tenant.key)}`);
  let current = $derived(
    entries.find(
      ([, suffix]) =>
        suffix && (pathname === base + suffix || pathname.startsWith(base + suffix + '/'))
    )?.[0] ?? 'Home'
  );
</script>

<a class="skip" href="#main">Skip to content</a>
<div class="app-shell">
  <aside>
    <a class="wordmark" href={base}><img src={logo} alt="" width="28" height="28" />bohmian</a>
    <button class="app-control mobile-menu" type="button" aria-controls="primary-navigation" aria-expanded={navOpen} onclick={() => navOpen = !navOpen}>Navigation</button>
    <nav id="primary-navigation" aria-label="Primary" class:collapsed={!navOpen}>
      {#each entries as [label, suffix] (label)}
        <a href={base + suffix} aria-current={current === label ? 'page' : undefined}>{label}</a>
      {/each}
    </nav>
  </aside>
  <div class="workspace">
    <header>
      <span class="product">WYRD</span><span class="context">{tenant.key} / <strong>{current}</strong></span>
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
  aside {
    background: var(--surface);
    border-right: 2px solid var(--border);
    padding: 16px 14px;
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
    .app-shell {
      grid-template-columns: minmax(0, 1fr);
    }
    aside {
      border-right: 0;
      border-bottom: 2px solid var(--border);
      padding: 14px 18px;
    }
    .wordmark {
      margin-bottom: 12px;
    }
    .mobile-menu {
      display: block;
    }
    nav {
      margin-top: 16px;
    }
    nav.collapsed {
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
