<script lang="ts">
  import Pill from '$lib/Pill.svelte';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();
  type Admin = (typeof data.admins)[number];
  /** How this principal proves itself: the root key, the platform identity provider, or a platform password. */
  const method = (admin: Admin) =>
    admin.principal_kind === 'global_admin' ? 'Root key' : admin.match_claim ? (admin.subject ? 'Platform SSO' : 'Platform SSO · not signed in yet') : 'Password';
</script>

<header class="pagehead"><div><h1>Admins</h1><p>People who can change this deployment.</p></div></header>

<div class="tablewrap">
  <table>
    <thead><tr><th>Name</th><th>Signs in with</th><th>Status</th><th><span class="sr-only">Session</span></th></tr></thead>
    <tbody>
      {#each data.admins as admin (admin.principal_id)}
        {@const you = admin.principal_id === data.session.principal_id}
        <tr>
          <td><b data-testid={you ? 'operator' : undefined}>{admin.name}</b>{#if you}{' '}<span class="muted">(you)</span>{/if}</td>
          <td>{method(admin)}</td>
          <td><Pill tone={admin.status === 'active' ? 'ok' : 'off'} label={admin.status === 'active' ? 'Active' : 'Suspended'} /></td>
          <td class="right">
            {#if you}
              <form method="POST" action="/platform/ui/logout"><button class="app-control" type="submit">Sign out</button></form>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
