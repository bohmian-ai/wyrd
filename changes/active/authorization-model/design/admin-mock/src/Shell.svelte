<!-- The existing rail, plus a name block with the mode control. Tenant and platform share it. -->
<script>
  import Icon from './Icon.svelte';
  import { I } from './icons.js';
  import logo from '../../../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/logo.svg';
  let { kind, path, mode, toggleMode, children } = $props();
  const t = '#/t/gtest';
  const tenantGroups = [
    [['Home', t, I.home], ['Cards', `${t}/cards`, I.cards], ['Repositories', `${t}/repositories`, I.repositories]],
    [['Observe', `${t}/observe`, I.observe], ['Verify', `${t}/verify`, I.verify], ['Query', `${t}/query`, I.query]],
  ];
  const platformGroups = [[
    ['Overview', '#/platform/ui', I.overview], ['Workspaces', '#/platform/ui/workspaces', I.workspaces],
    ['Sign-in providers', '#/platform/ui/providers', I.providers], ['Mail', '#/platform/ui/mail', I.mail],
    ['Admins', '#/platform/ui/admins', I.admins], ['Stored secrets', '#/platform/ui/secrets', I.secrets],
  ]];
  const groups = $derived(kind === 'platform' ? platformGroups : tenantGroups);
  const here = (href) => '#' + path === href;
</script>

<div class="shell">
  <aside class="rail" aria-label="Primary">
    <a class="brand" href={kind === 'platform' ? '#/platform/ui' : t}><img src={logo} alt="" width="22" height="22" />Wyrd{#if kind === 'platform'}<span class="muted" style="font-weight:400;font-size:12px"> platform</span>{/if}</a>
    {#each groups as group, i (i)}
      {#if i}<hr />{/if}
      <nav aria-label="Destinations">
        {#each group as [label, href, icon] (href)}
          <a class="item" {href} aria-current={here(href) ? 'page' : undefined}><Icon d={icon} />{label}</a>
        {/each}
      </nav>
    {/each}
    <div class="foot">
      {#if kind !== 'platform'}
        <a class="item" href="{t}/settings/people" aria-current={path.startsWith('/t/gtest/settings') ? 'page' : undefined}><Icon d={I.settings} />Settings</a>
      {/if}
      <div class="me">
        <span class="avatar" aria-hidden="true">{kind === 'platform' ? 'OP' : 'SF'}</span>
        <a class="who" href={kind === 'platform' ? '#/platform/ui' : `${t}/account`} style="text-decoration:none">
          <b>{kind === 'platform' ? 'operator' : 'Steven F.'}</b>
          <span>{kind === 'platform' ? 'Platform admin' : 'steven@example.test'}</span>
        </a>
        <button class="iconbtn" aria-label="Switch to {mode === 'dark' ? 'light' : 'dark'} mode" onclick={toggleMode}><Icon d={mode === 'dark' ? I.sun : I.moon} /></button>
      </div>
    </div>
  </aside>
  {@render children()}
</div>
