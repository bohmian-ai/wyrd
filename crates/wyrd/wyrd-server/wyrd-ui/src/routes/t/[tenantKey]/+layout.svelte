<script lang="ts">
  import type { LayoutProps } from './$types';
  import { toggleMode, theme } from '$lib/theme.svelte';
  import logo from '../../../../brand/logo.svg?inline';
  let { data, children }: LayoutProps = $props();
  let base = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
</script>

<header>
  <a class="brand" href={base}><img src={logo} alt="" width="28" height="28" />Wyrd</a>
  <nav aria-label="Primary"><a href={base}>Home</a><a href={`${base}/settings`}>Settings</a></nav>
  <div class="identity">
    {#if data.session.tenants.length > 1}
      <details>
        <summary aria-label="Current tenant">{data.tenant.name}</summary>
        <form class="menu" method="POST" action="/?/switch">
          {#each data.session.tenants as tenant (tenant.key)}
            <button class="app-control" type="submit" name="tenantKey" value={tenant.key}>{tenant.name}</button>
          {/each}
        </form>
      </details>
    {:else}
      <span aria-label="Current tenant">{data.tenant.name}</span>
    {/if}
    <span>{data.session.subject.name}</span>
    <form method="POST" action="/?/logout">
      <input type="hidden" name="tenantKey" value={data.tenant.key} />
      <button class="app-control" type="submit">Sign out</button>
    </form>
    <button class="app-control" type="button" onclick={toggleMode} aria-label={`Switch to ${theme.mode === 'dark' ? 'light' : 'dark'} mode`}>◐ {theme.mode === 'dark' ? 'Light' : 'Dark'}</button>
  </div>
</header>
<main id="main">{@render children()}</main>

<style>
  header {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 16px;
    padding: 10px 24px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    font: 12px var(--font-sans);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text);
    font: 600 18px var(--font-sans);
    text-decoration: none;
  }
  img {
    border-radius: var(--r);
  }
  nav {
    display: flex;
    gap: 12px;
  }
  nav a {
    color: var(--declare);
    text-decoration: none;
  }
  .identity {
    margin-left: auto;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
  }
  details {
    position: relative;
  }
  summary {
    cursor: pointer;
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 8px);
    z-index: 5;
    display: grid;
    gap: 4px;
    min-width: 200px;
    padding: 8px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--r);
  }
  .menu button {
    text-align: left;
  }
  main {
    padding: 24px;
  }
  @media (max-width: 767px) {
    header,
    main {
      padding-left: 16px;
      padding-right: 16px;
    }
  }
</style>
