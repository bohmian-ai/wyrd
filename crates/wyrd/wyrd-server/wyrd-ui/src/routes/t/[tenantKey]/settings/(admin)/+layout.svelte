<!--
  Tenant Settings: the administration section list beside one work region.
  Your account is not a section; it opens from the name block.
-->
<script lang="ts">
  import '$lib/console.css';
  import { page } from '$app/state';
  import { ICONS } from '$lib/icons';
  import Icon from '$lib/Icon.svelte';
  import type { LayoutProps } from './$types';
  let { data, children }: LayoutProps = $props();
  const base = $derived(`/t/${encodeURIComponent(data.tenant.key)}/settings`);
  const groups = $derived([
    [
      'Access',
      [
        ['people', 'People', ICONS.people, data.counts.people],
        ['service-accounts', 'Service accounts', ICONS.service, data.counts.services],
        ['roles', 'Roles', ICONS.roles, data.counts.roles]
      ]
    ],
    [
      'Security',
      [
        ['sign-in', 'Sign-in', ICONS.signin, null],
        ['secrets', 'Secrets', ICONS.secrets, null]
      ]
    ],
    ['Workspace', [['integrations', 'Integrations', ICONS.integrations, null]]]
  ] as const);
  const here = (key: string) => page.url.pathname === `${base}/${key}` || page.url.pathname.startsWith(`${base}/${key}/`);
</script>

<div class="ui-admin">
  <div class="admin">
    <nav class="sections" aria-label="Settings">
      {#each groups as [title, items] (title)}
        <div class="group">
          <h2>{title}</h2>
          {#each items as [key, label, icon, count] (key)}
            <a href={`${base}/${key}`} aria-current={here(key) ? 'page' : undefined}>
              <Icon d={icon} />{label}
              {#if count !== null}<span class="count">{count}</span>{/if}
            </a>
          {/each}
        </div>
      {/each}
    </nav>
    <div class="work">{@render children()}</div>
  </div>
</div>
