<!--
  The signed-in shell shared by tenant pages and the platform console: a
  persistent left rail with its destinations, an optional Settings link, and
  the name block, which opens the signed-in person's own page and carries the
  light/dark control. Destinations not delivered yet stay visible but
  disabled. Below 768px the rail becomes a top bar.
-->
<script lang="ts" module>
  /** One rail destination; `href` absent means not delivered yet. */
  export interface Destination {
    readonly label: string;
    readonly href?: string;
    readonly icon: string;
  }
  /** Who is signed in, as the name block shows them: a label, its second line and their own page. */
  export interface NameBlock {
    readonly name: string;
    readonly detail: string;
    readonly href: string;
  }
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';
  import { page } from '$app/state';
  import { toggleMode, theme } from '$lib/theme.svelte';
  import { ICONS } from '$lib/icons';
  import Icon from '$lib/Icon.svelte';
  import logo from '../../brand/logo.svg?inline';

  interface Props {
    /** Where the brand links; also the destination matched only exactly. */
    home: string;
    /** A plane word beside the wordmark, such as "platform". */
    plane?: string;
    groups: readonly (readonly Destination[])[];
    /** The Settings link above the name block; absent on the platform console. */
    settings?: string;
    /** Show the Find-or-ask control, which is not available yet. */
    find?: boolean;
    me: NameBlock;
    children: Snippet;
  }
  let { home, plane, groups, settings, find = false, me, children }: Props = $props();
  let collapsed = $state(false);

  const current = (href: string) => (href === home ? page.url.pathname === home : page.url.pathname.startsWith(href));
  /** Up to two initials of the label, or its first character. */
  const initials = $derived(
    (/\s/.test(me.name.trim()) ? me.name.trim().split(/\s+/).map((part) => part[0]).join('') : me.name.trim()).slice(0, 2).toUpperCase()
  );
</script>

<div class="shell" class:collapsed>
  <aside aria-label="Primary">
    <div class="top">
      <a class="brand" href={home}><img src={logo} alt="" width="22" height="22" /><span class="label">Wyrd{#if plane}{' '}<span class="plane">{plane}</span>{/if}</span></a>
      <button type="button" class="collapse" aria-label={collapsed ? 'Expand navigation' : 'Collapse navigation'} aria-expanded={!collapsed} onclick={() => (collapsed = !collapsed)}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d={collapsed ? 'M9 6l6 6-6 6' : 'M15 6l-6 6 6 6'} /></svg>
      </button>
    </div>
    {#if find}
      <button type="button" class="find" disabled title="Ask is not available yet">⌕ <span class="label">Find or ask…</span> <kbd>⌘K</kbd></button>
    {/if}
    <nav aria-label="Destinations">
      {#each groups as destinations, index (index)}
        {#if index > 0}<div class="rule" aria-hidden="true"></div>{/if}
        {#each destinations as destination (destination.label)}
          {#if destination.href}
            <a class="item" href={destination.href} aria-current={current(destination.href) ? 'page' : undefined} title={destination.label}>
              <Icon d={destination.icon} />
              <span class="label">{destination.label}</span>
            </a>
          {:else}
            <span class="item" aria-disabled="true" title="{destination.label} is not available yet">
              <Icon d={destination.icon} />
              <span class="label">{destination.label}</span>
            </span>
          {/if}
        {/each}
      {/each}
    </nav>
    <div class="foot">
      {#if settings}
        <a class="item" href={settings} aria-current={current(settings) && !current(me.href) ? 'page' : undefined} title="Settings">
          <Icon d={ICONS.settings} />
          <span class="label">Settings</span>
        </a>
      {/if}
      <div class="me">
        <span class="avatar" aria-hidden="true">{initials}</span>
        <a class="who label" href={me.href} aria-current={current(me.href) ? 'page' : undefined}>
          <b>{me.name}</b>
          <span>{me.detail}</span>
        </a>
        <button type="button" class="iconbtn" aria-label={`Switch to ${theme.mode === 'dark' ? 'light' : 'dark'} mode`} onclick={toggleMode}>
          <Icon d={theme.mode === 'dark' ? ICONS.sun : ICONS.moon} />
        </button>
      </div>
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
  .plane {
    color: var(--muted);
    font-weight: 400;
    font-size: 12px;
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
    font: 14px var(--font-sans);
    text-decoration: none;
  }
  .item :global(svg) {
    flex: none;
  }
  a.item:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .item[aria-current='page'] {
    background: var(--declare-soft);
    color: var(--text);
    font-weight: var(--weight-strong);
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
  .me {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 6px;
    padding: 8px 10px;
    border-top: 1px solid var(--border-soft);
  }
  .avatar {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--declare-soft);
    color: var(--text);
    font: 600 11px var(--font-sans);
  }
  .who {
    min-width: 0;
    color: var(--text);
    line-height: 1.25;
    text-decoration: none;
  }
  .who b {
    display: block;
    overflow: hidden;
    font-size: 13px;
    font-weight: var(--weight-strong);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .who span {
    display: block;
    overflow: hidden;
    color: var(--muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .who:hover b {
    text-decoration: underline;
  }
  .iconbtn {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    margin-left: auto;
    border: 1px solid transparent;
    border-radius: var(--r);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .iconbtn:hover {
    border-color: var(--border);
    color: var(--text);
  }
  .collapsed .me {
    flex-direction: column;
    padding: 8px 0;
  }
  .collapsed .iconbtn {
    margin-left: 0;
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
    .me {
      margin: 0;
      padding: 0 0 0 6px;
      border: 0;
    }
  }
</style>
