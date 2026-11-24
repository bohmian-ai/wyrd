<script lang="ts">
  import '$lib/console.css';
  import { page } from '$app/state';
  import PasswordInput from '$lib/PasswordInput.svelte';
  import Pill from '$lib/Pill.svelte';
  import RecoveryEmail from '$lib/RecoveryEmail.svelte';
  import { guidance } from '$lib/Entry.svelte';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  let dismissed = $state<string | null>(null);
  let copied = $state(false);
  const issued = $derived(form?.issued && form.issued.id !== dismissed ? form.issued : null);
  const problem = $derived(form && 'problem' in form ? form.problem : null);
  const when = (value: string | null | undefined) => (value ? new Date(value).toISOString().slice(0, 16).replace('T', ' ') : '—');
  const keyState = (key: { revoked_at: string | null; expires_at: string | null }) =>
    key.revoked_at
      ? ({ tone: 'off', label: 'Revoked' } as const)
      : key.expires_at && Date.parse(key.expires_at) <= Date.now()
        ? ({ tone: 'off', label: 'Expired' } as const)
        : ({ tone: 'ok', label: 'Active' } as const);
  /** The word for how the account signs in, as the server names it. */
  const PROVIDER = { google: 'Google', github: 'GitHub', sso: 'Single sign-on', password: 'Password' } as const;
  const FROM = { google: 'Google', github: 'GitHub', sso: 'your company' } as const;
  async function copy(secret: string) {
    await navigator.clipboard.writeText(secret);
    copied = true;
  }
</script>

<svelte:head><title>Your account · {data.tenant.name} · Wyrd</title></svelte:head>
<div class="ui-admin">
<div class="work narrow">
  <header class="pagehead"><div><h1>Your account</h1><p>How you sign in to {data.tenant.name}.</p></div></header>

  {#if problem}
    <div class="app-problem" role="alert" data-state={problem.status === 403 ? 'unauthorized' : 'error'}>
      <strong>{problem.status === 403 ? '⊘' : '✕'} {problem.title}</strong>
      <p>{guidance(problem)}</p>
      <code>{problem.code}</code>
    </div>
  {/if}

  {#if data.problem?.status === 403}
    <div class="app-problem" data-state="unauthorized">
      <strong>⊘ Account settings are for Users</strong>
      <p>This session belongs to a Service account; manage its keys from tenant administration.</p>
      <code>{data.problem.code}</code>
    </div>
  {:else if data.problem || !data.profile || !data.keys}
    <div class="app-problem" data-state="error">
      <strong>✕ Account settings are unavailable</strong>
      <p>{data.problem?.remediation ?? 'Please try again later.'}</p>
      <code>{data.problem?.code}</code>
    </div>
  {:else}
    {@const profile = data.profile}
    {@const method = profile.authentication}
    <section class="card" aria-label="Profile" data-testid="profile">
      <div class="row">
        <div class="label"><b>Name</b><span>{method === 'password' ? 'Your username.' : `From ${FROM[method]}; updates each time you sign in.`}</span></div>
        <div class="ctl">{profile.display_name ?? profile.username ?? profile.email ?? '—'}</div>
      </div>
      {#if profile.email}<div class="row"><div class="label"><b>Email</b></div><div class="ctl">{profile.email}</div></div>{/if}
      <div class="row">
        <div class="label"><b>Signs in with</b>{#if method !== 'password'}<span>Wyrd never links accounts by matching email.</span>{/if}</div>
        <div class="ctl"><Pill tone="ok" label={PROVIDER[method]} /></div>
      </div>
    </section>

    {#if method === 'password'}
      <section class="card" aria-labelledby="password">
        <header><h2 id="password">Password</h2></header>
        <form class="row" method="POST" action="?/password">
          <div class="field grow">
            <label for="current-password">Current password</label>
            <PasswordInput id="current-password" name="currentPassword" required autocomplete="current-password" />
            <label for="new-password">New password</label>
            <PasswordInput id="new-password" name="newPassword" required autocomplete="new-password" />
            <span class="hint">Changing it signs you out everywhere you use this password.</span>
          </div>
          <div class="ctl"><button class="app-control" type="submit">Change password</button></div>
        </form>
      </section>
    {/if}

    {#if data.recovery}<div class="recovery"><RecoveryEmail tenant={profile.tenant} /></div>{/if}

    <section class="card" aria-labelledby="keys">
      <header><h2 id="keys">API keys</h2></header>
      {#if issued}
        <div class="row secret" role="status">
          <div class="field grow">
            <b>✓ Key created. Copy it now; it is not shown again.</b>
            <code class="mono" data-secret>{issued.credential}</code>
          </div>
          <div class="ctl">
            <button class="app-control" type="button" onclick={() => copy(issued.credential)}>{copied ? 'Copied' : 'Copy'}</button>
            <button class="app-control" type="button" onclick={() => ((dismissed = issued.id), (copied = false))}>Dismiss</button>
          </div>
        </div>
      {/if}
      {#if form && 'revoked' in form}<p class="row" role="status">✓ Key revoked.</p>{/if}
      <form class="row" method="POST" action="?/create">
        <div class="field grow">
          <label for="key-name">Name</label>
          <input id="key-name" class="app-input" name="name" required maxlength="64" autocomplete="off" />
          <label for="key-days">Expires after (days)</label>
          <input id="key-days" class="app-input" style:width="80px" type="number" name="days" required min="1" max="365" value="30" />
          <fieldset>
            <legend>Permissions this key may use</legend>
            {#each profile.permissions as permission (permission)}
              <label class="check"><input type="checkbox" name="permission" value={permission} /> <span class="mono">{permission}</span></label>
            {/each}
          </fieldset>
          <span class="hint">A key acts as you, limited to the permissions you choose and to what you hold when it is used. Creating one needs a sign-in in the last five minutes.</span>
        </div>
        <div class="ctl"><button class="app-control primary" type="submit">Create key</button></div>
      </form>
      {#if data.keys.credentials.length}
        <div class="keys">
          <table>
            <thead><tr><th>Name</th><th>Prefix</th><th>Permissions</th><th>Created</th><th>Expires</th><th>Last used</th><th>Status</th><th><span class="sr-only">Revoke</span></th></tr></thead>
            <tbody>
              {#each data.keys.credentials as key (key.id)}
                {@const state = keyState(key)}
                <tr>
                  <td><b>{key.name ?? '—'}</b></td>
                  <td class="mono">{key.prefix}</td>
                  <td class="mono">{key.permission_ceiling?.join(', ') ?? '—'}</td>
                  <td class="muted">{when(key.created_at)}</td>
                  <td class="muted">{when(key.expires_at)}</td>
                  <td class="muted">{when(key.last_used_at)}</td>
                  <td><Pill tone={state.tone} label={state.label} /></td>
                  <td class="right">
                    {#if !key.revoked_at}
                      <form method="POST" action="?/revoke">
                        <input type="hidden" name="id" value={key.id} />
                        <button class="app-control" type="submit" aria-label="Revoke {key.name ?? key.prefix}">Revoke</button>
                      </form>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <div class="row"><span class="muted">– No keys yet</span></div>
      {/if}
      {#if page.url.searchParams.has('after') || data.keys.next_cursor}
        <div class="row">
          {#if page.url.searchParams.has('after')}<a href="?">Newest keys</a>{/if}
          {#if data.keys.next_cursor}<a href="?after={encodeURIComponent(data.keys.next_cursor)}">Older keys</a>{/if}
        </div>
      {/if}
    </section>
  {/if}

  <section class="card" aria-labelledby="session">
    <header><h2 id="session">Session</h2></header>
    {#if data.session.tenants.length > 1}
      <form class="row" method="POST" action="/?/switch">
        <div class="label"><b>Switch workspace</b><span>You are signed in to {data.session.tenants.length} workspaces.</span></div>
        <div class="ctl wrap">
          {#each data.session.tenants as tenant (tenant.key)}
            <button class="app-control" type="submit" name="tenantKey" value={tenant.key} aria-current={tenant.key === data.tenant.key ? 'true' : undefined}>{tenant.name}</button>
          {/each}
        </div>
      </form>
    {/if}
    <form class="row" method="POST" action="/?/logout">
      <div class="label"><b>Sign out</b><span>Ends this session in {data.tenant.name}.</span></div>
      <input type="hidden" name="tenantKey" value={data.tenant.key} />
      <div class="ctl"><button class="app-control" type="submit">Sign out</button></div>
    </form>
  </section>
</div>
</div>

<style>
  .grow {
    flex: 1;
    max-width: 480px;
  }
  form.row {
    align-items: flex-end;
  }
  .wrap {
    flex-wrap: wrap;
    gap: 8px;
  }
  .wrap [aria-current='true'] {
    border-color: var(--declare);
  }
  .recovery {
    margin-bottom: 24px;
  }
  .secret {
    border-left: 2px solid var(--declare);
  }
  .secret code {
    overflow-wrap: anywhere;
  }
  fieldset {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
  }
  legend {
    font-size: 12px;
    color: var(--muted);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 400;
  }
  .keys {
    overflow-x: auto;
    border-top: 1px solid var(--border-soft);
  }
</style>
