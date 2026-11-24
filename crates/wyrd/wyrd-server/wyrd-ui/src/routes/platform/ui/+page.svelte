<script lang="ts">
  import { getContext } from 'svelte';
  import { enhance } from '$app/forms';
  import ConsoleProblem from '$lib/ConsoleProblem.svelte';
  import type { ConsoleReauth } from '$lib/console-reauth.svelte';
  import { utc } from '$lib/platform-copy';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  const reauth = getContext<ConsoleReauth>('console-reauth');
  const status: Record<string, string> = { active: '✓ active', suspended: '⊘ suspended', provisioning: '◌ provisioning', failed: '✕ failed' };
</script>

{#if form?.problem && form.problem.code !== 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED'}<ConsoleProblem issue={form.problem} />{/if}
<div role="status" aria-live="polite">{#if form?.notice}<p class="done" data-testid="notice"><span class="ok">✓</span> {form.notice}</p>{/if}</div>

<section aria-labelledby="session">
  <h2 id="session">This session</h2>
  <dl>
    <dt>Operator</dt><dd>{data.session.name}</dd>
    <dt>Signed in with</dt><dd>{data.session.method === 'password' ? 'Platform password' : 'Platform SSO'}</dd>
    <dt>Idle timeout</dt><dd>{utc(data.session.idle_expires_at)}</dd>
    <dt>Session ends</dt><dd>{utc(data.session.expires_at)}</dd>
    <dt>Fresh sign-in</dt><dd data-testid="recent">{data.session.recent ? '✓ yes · changes apply at once' : '– no · changes ask you to confirm'}</dd>
  </dl>
</section>
<section aria-labelledby="workspaces">
  <h2 id="workspaces">Workspaces</h2>
  <div class="scroll" role="region" aria-label="Workspaces" tabindex="-1">
    <table>
      <thead><tr><th>Workspace</th><th>Key</th><th>Status</th><th></th></tr></thead>
      <tbody>
        {#each data.tenants as tenant (tenant.id)}
          <tr>
            <td>{tenant.display_name}</td>
            <td><code>{tenant.slug}</code></td>
            <td data-testid="status-{tenant.slug}">{status[tenant.status] ?? tenant.status}</td>
            <td>
              {#if tenant.status === 'active' || tenant.status === 'suspended'}
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
  <p>Suspending keeps every row, grant and credential; resuming restores them. Provisioning and failed workspaces change only by provisioning.</p>
</section>

<style>
  .scroll {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: 12px var(--font-sans);
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-soft);
    text-align: left;
    vertical-align: middle;
    white-space: nowrap;
  }
  th {
    font: 700 11px var(--font-mono);
    color: var(--muted);
  }
</style>
