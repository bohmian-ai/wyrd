<!--
  The signed-in shell: a persistent left rail with the tenant's destinations,
  the identity menu (tenant switch, sign out, theme) and the page beside it.
  Destinations this release does not deliver, and Ask, stay visible but
  disabled. Below 768px the rail becomes a top bar.
-->
<script lang="ts">
  import { page } from '$app/state';
  import type { LayoutProps } from './$types';
  import { toggleMode, theme } from '$lib/theme.svelte';
  import logo from '../../../../brand/logo.svg?inline';
  let { data, children }: LayoutProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  let collapsed = $state(false);

  /** One rail destination; `href` absent means not delivered yet. */
  interface Destination {
    readonly label: string;
    readonly href?: string;
    readonly icon: string;
  }
  const ICONS = {
    home: 'M3 11l9-7 9 7v9h-6v-6H9v6H3z',
    cards: 'M4 4h16v16H4zM4 9h16',
    observe: 'M3 12h4l3-7 4 14 3-7h4',
    verify: 'M5 12l5 5L20 7',
    query: 'M4 6c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6',
    changes: 'M6 8v8M8 6h3a5 5 0 0 1 5 5M4 6a2 2 0 1 0 4 0 2 2 0 1 0-4 0M4 18a2 2 0 1 0 4 0 2 2 0 1 0-4 0M16 12a2 2 0 1 0 4 0 2 2 0 1 0-4 0',
    settings: 'M12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1'
  } as const;
  const groups = $derived<readonly (readonly Destination[])[]>([
    [
      { label: 'Home', href: base, icon: ICONS.home },
      { label: 'Cards', icon: ICONS.cards }
    ],
    [
      { label: 'Observe', href: `${base}/observe`, icon: ICONS.observe },
      { label: 'Verify', icon: ICONS.verify },
      { label: 'Query', href: `${base}/query`, icon: ICONS.query }
    ],
    [{ label: 'Changes', icon: ICONS.changes }]
  ]);
  const current = (href: string) => (href === base ? page.url.pathname === base : page.url.pathname.startsWith(href));
</script>

<div class="shell" class:collapsed>
  <aside aria-label="Primary">
    <div class="top">
      <a class="brand" href={base}><img src={logo} alt="" width="22" height="22" /><span class="label">Wyrd</span></a>
      <button type="button" class="collapse" aria-label={collapsed ? 'Expand navigation' : 'Collapse navigation'} aria-expanded={!collapsed} onclick={() => (collapsed = !collapsed)}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d={collapsed ? 'M9 6l6 6-6 6' : 'M15 6l-6 6 6 6'} /></svg>
      </button>
    </div>
    <button type="button" class="find" disabled title="Ask is not available yet">⌕ <span class="label">Find or ask…</span> <kbd>⌘K</kbd></button>
    <nav aria-label="Destinations">
      {#each groups as destinations, index (index)}
        {#if index > 0}<div class="rule" aria-hidden="true"></div>{/if}
        {#each destinations as destination (destination.label)}
          {#if destination.href}
            <a class="item" href={destination.href} aria-current={current(destination.href) ? 'page' : undefined} title={destination.label}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d={destination.icon} /></svg>
              <span class="label">{destination.label}</span>
            </a>
          {:else}
            <span class="item" aria-disabled="true" title="{destination.label} is not available yet">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d={destination.icon} /></svg>
              <span class="label">{destination.label}</span>
            </span>
          {/if}
        {/each}
      {/each}
    </nav>
    <div class="foot">
      <a class="item" href={`${base}/settings`} aria-current={current(`${base}/settings`) ? 'page' : undefined} title="Settings">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d={ICONS.settings} /></svg>
        <span class="label">Settings</span>
      </a>
      <details class="identity">
        <summary aria-label="Account and tenant">
          <span class="avatar" aria-hidden="true">{data.session.subject.name.slice(0, 1).toLowerCase()}</span>
          <span class="who label"><span>{data.session.subject.name}</span><span class="tenant" aria-label="Current tenant">{data.tenant.name} ▾</span></span>
        </summary>
        <div class="menu">
          {#if data.session.tenants.length > 1}
            <form method="POST" action="/?/switch">
              <p>Switch tenant</p>
              {#each data.session.tenants as tenant (tenant.key)}
                <button class="app-control" type="submit" name="tenantKey" value={tenant.key} aria-current={tenant.key === data.tenant.key ? 'true' : undefined}>{tenant.name}</button>
              {/each}
            </form>
          {/if}
          <a class="app-control" href={`${base}/settings/account`}>Account</a>
          <button class="app-control" type="button" onclick={toggleMode} aria-label={`Switch to ${theme.mode === 'dark' ? 'light' : 'dark'} mode`}>◐ {theme.mode === 'dark' ? 'Light' : 'Dark'} mode</button>
          <form method="POST" action="/?/logout">
            <input type="hidden" name="tenantKey" value={data.tenant.key} />
            <button class="app-control" type="submit">Sign out</button>
          </form>
        </div>
      </details>
    </div>
  </aside>
  <main id="main">{@render children()}</main>
</div>

<style>
  .shell {
    display: flex;
    min-height: 100vh;
  }
  aside {
    position: sticky;
    top: 0;
    flex: 0 0 212px;
    min-width: 0;
    box-sizing: border-box;
    height: 100vh;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 10px;
    border-right: 1px solid var(--border);
    background: var(--nav-surface);
  }
  .collapsed aside {
    flex-basis: 56px;
  }
  .collapsed .label,
  .collapsed kbd {
    display: none;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 6px 12px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text);
    font: 600 15px var(--font-sans);
    text-decoration: none;
  }
  .brand img {
    border-radius: var(--r);
  }
  .collapse {
    width: 28px;
    height: 28px;
    border: 0;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .find {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    margin-bottom: 10px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--bg);
    color: var(--muted);
    font: 12px var(--font-mono);
  }
  .find:disabled {
    cursor: not-allowed;
    opacity: 0.7;
  }
  kbd {
    margin-left: auto;
    color: var(--muted);
    font: inherit;
  }
  nav,
  .foot {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .foot {
    margin-top: auto;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--r);
    color: var(--muted);
    font: 13px var(--font-sans);
    text-decoration: none;
  }
  a.item:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .item[aria-current='page'] {
    background: var(--declare-soft);
    box-shadow: inset 2px 0 0 var(--declare);
    color: var(--text);
  }
  .item[aria-disabled='true'] {
    cursor: not-allowed;
    opacity: 0.55;
  }
  .rule {
    height: 1px;
    margin: 8px 4px;
    background: var(--border-soft);
  }
  .identity {
    position: relative;
    margin-top: 8px;
    padding-top: 10px;
    border-top: 1px solid var(--border-soft);
  }
  summary {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    list-style: none;
    color: var(--text);
    font: 12px var(--font-mono);
    cursor: pointer;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .avatar {
    display: inline-flex;
    flex: 0 0 22px;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--declare-soft);
    color: var(--declare);
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .who span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tenant {
    color: var(--muted);
    font-size: 11px;
  }
  .menu {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 0;
    z-index: 20;
    display: grid;
    gap: 6px;
    min-width: 200px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  .menu form {
    display: grid;
    gap: 4px;
  }
  .menu p {
    margin: 0;
    color: var(--muted);
    font: 11px var(--font-mono);
    text-transform: uppercase;
  }
  .menu button {
    text-align: left;
  }
  main {
    flex: 999 1 0;
    min-width: 0;
  }
  @media (max-width: 767px) {
    .shell {
      flex-direction: column;
    }
    aside {
      position: static;
      flex: none;
      height: auto;
      flex-flow: row wrap;
      align-items: center;
      gap: 4px;
      padding: 8px 16px;
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
    .top {
      padding: 0;
    }
    .collapse,
    .find,
    .rule {
      display: none;
    }
    nav,
    .foot {
      flex-direction: row;
      flex-wrap: wrap;
      margin: 0;
    }
    .identity {
      margin: 0;
      padding: 0;
      border: 0;
    }
    .menu {
      top: calc(100% + 8px);
      right: 0;
      bottom: auto;
      left: auto;
    }
  }
</style>
