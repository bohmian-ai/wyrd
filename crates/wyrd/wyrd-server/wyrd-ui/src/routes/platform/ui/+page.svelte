<script lang="ts">
  import { utc } from '$lib/platform-copy';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();
  const status: Record<string, string> = { active: '✓ active', suspended: '⊘ suspended', provisioning: '◌ provisioning', failed: '✕ failed' };
</script>

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
  <div class="scroll">
    <table>
      <thead><tr><th>Workspace</th><th>Key</th><th>Status</th><th></th></tr></thead>
      <tbody>
        {#each data.tenants as tenant (tenant.id)}
          <tr>
            <td>{tenant.display_name}</td>
            <td><code>{tenant.slug}</code></td>
            <td data-testid="status-{tenant.slug}">{status[tenant.status] ?? tenant.status}</td>
            <td></td>
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
