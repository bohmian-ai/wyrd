<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import Pill from '$lib/Pill.svelte';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  const tones = { active: 'ok', suspended: 'off', provisioning: 'info', failed: 'danger' } as const;
  const tone = (status: string) => tones[status as keyof typeof tones] ?? 'off';
</script>

<header class="pagehead"><div><h1>Workspaces</h1><p>Every tenant on this deployment.</p></div></header>

{#if form?.problem && form.problem.code !== 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED'}<ConsoleProblem issue={form.problem} />{/if}
<div role="status" aria-live="polite">{#if form?.notice}<p class="done" data-testid="notice"><span class="ok">✓</span> {form.notice}</p>{/if}</div>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -- a scrolling table must be reachable by keyboard -->
<div class="tablewrap" role="region" aria-label="Workspaces" tabindex="0">
  <table>
    <thead><tr><th>Workspace</th><th>Key</th><th>Status</th><th><span class="sr-only">Change</span></th></tr></thead>
    <tbody>
      {#each data.tenants as tenant (tenant.id)}
        <tr>
          <td><b>{tenant.display_name}</b></td>
          <td class="mono">{tenant.slug}</td>
          <td data-testid="status-{tenant.slug}"><Pill tone={tone(tenant.status)} label={tenant.status[0].toUpperCase() + tenant.status.slice(1)} /></td>
          <td class="right">
            {#if tenant.slug !== 'wyrd-system' && (tenant.status === 'active' || tenant.status === 'suspended')}
              {@const next = tenant.status === 'active' ? 'suspended' : 'active'}
              <form method="POST" action="?/status" use:enhance={reauth.guard(`${next === 'suspended' ? 'Suspending' : 'Resuming'} ${tenant.slug}`)}>
                <input type="hidden" name="id" value={tenant.id} />
                <input type="hidden" name="slug" value={tenant.slug} />
                <input type="hidden" name="status" value={next} />
                <button class="app-control" type="submit">{next === 'suspended' ? 'Suspend' : 'Resume'}</button>
              </form>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
<p class="foot">Suspending keeps every row, grant and credential; resuming restores them. The platform's own <span class="mono">wyrd-system</span> workspace never suspends.</p>

<style>
  .foot {
    margin-top: 12px;
    color: var(--muted);
    font-size: 13px;
  }
</style>
