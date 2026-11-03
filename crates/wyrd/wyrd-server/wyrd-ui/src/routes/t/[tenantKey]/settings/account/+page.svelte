<script lang="ts">
  import { page } from '$app/state';
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  let dismissed = $state<string | null>(null);
  let copied = $state(false);
  const issued = $derived(form?.issued && form.issued.id !== dismissed ? form.issued : null);
  const problem = $derived(form && 'problem' in form ? form.problem : null);
  const when = (value: string | null | undefined) => (value ? new Date(value).toISOString().slice(0, 16).replace('T', ' ') : '—');
  const keyState = (key: { revoked_at: string | null; expires_at: string | null }) =>
    key.revoked_at ? '⊘ revoked' : key.expires_at && Date.parse(key.expires_at) <= Date.now() ? '– expired' : '✓ active';
  async function copy(secret: string) {
    await navigator.clipboard.writeText(secret);
    copied = true;
  }
</script>

<svelte:head><title>Account · {data.tenant.name} · Wyrd</title></svelte:head>
<div class="heading">
  <h1>Account</h1>
  <p>Your identity in {data.tenant.name} and your own API keys.</p>
</div>

{#if problem}
  <div class="app-problem" role="alert" data-state={problem.status === 403 ? 'unauthorized' : 'error'}>
    <strong>{problem.status === 403 ? '⊘' : '✕'} {problem.title}</strong>
    <p>{problem.remediation}</p>
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
  <div class="grid">
    <section aria-labelledby="profile">
      <h2 id="profile">Profile</h2>
      <dl>
        <dt>Username</dt><dd>{profile.username ?? '—'}</dd>
        <dt>Sign-in</dt><dd>{profile.authentication === 'password' ? 'Username and password' : 'Single sign-on'}</dd>
        <dt>Principal</dt><dd>{profile.principal_id}</dd>
        <dt>Tenant</dt><dd>{profile.tenant}</dd>
      </dl>
    </section>

    {#if profile.authentication === 'password'}
      <section aria-labelledby="password">
        <h2 id="password">Password</h2>
        <form class="stack" method="POST" action="?/password">
          <label for="current-password">Current password</label>
          <input id="current-password" class="app-input" type="password" name="currentPassword" required autocomplete="current-password" />
          <label for="new-password">New password</label>
          <input id="new-password" class="app-input" type="password" name="newPassword" required minlength="15" maxlength="128" autocomplete="new-password" />
          <p>Changing it signs you out everywhere you use this password.</p>
          <button class="app-control" type="submit">Change password</button>
        </form>
      </section>
    {/if}

    <section aria-labelledby="keys">
      <h2 id="keys">API keys</h2>
      {#if issued}
        <div class="secret" role="status">
          <strong>✓ Key created. Copy it now; it is not shown again.</strong>
          <code data-secret>{issued.credential}</code>
          <div class="row">
            <button class="app-control" type="button" onclick={() => copy(issued.credential)}>{copied ? 'Copied' : 'Copy'}</button>
            <button class="app-control" type="button" onclick={() => ((dismissed = issued.id), (copied = false))}>Dismiss</button>
          </div>
        </div>
      {/if}
      {#if form && 'revoked' in form}<p role="status">✓ Key revoked.</p>{/if}
      <form class="stack" method="POST" action="?/create">
        <label for="key-name">Name</label>
        <input id="key-name" class="app-input" name="name" required maxlength="64" autocomplete="off" />
        <label for="key-days">Expires after (days)</label>
        <input id="key-days" class="app-input" type="number" name="days" required min="1" max="365" value="30" />
        <fieldset>
          <legend>Permissions this key may use</legend>
          {#each profile.permissions as permission (permission)}
            <label class="check"><input type="checkbox" name="permission" value={permission} /> <code>{permission}</code></label>
          {/each}
        </fieldset>
        <p>A key acts as you, limited to the permissions you choose and to what you hold when it is used. Creating one needs a sign-in in the last five minutes.</p>
        <button class="app-control primary" type="submit">Create key</button>
      </form>
      {#if data.keys.credentials.length}
        <table>
          <thead><tr><th>Name</th><th>Prefix</th><th>Permissions</th><th>Created</th><th>Expires</th><th>Last used</th><th>State</th><th></th></tr></thead>
          <tbody>
            {#each data.keys.credentials as key (key.id)}
              <tr>
                <td>{key.name ?? '—'}</td>
                <td><code>{key.prefix}</code></td>
                <td><code>{key.permission_ceiling?.join(', ') ?? '—'}</code></td>
                <td>{when(key.created_at)}</td>
                <td>{when(key.expires_at)}</td>
                <td>{when(key.last_used_at)}</td>
                <td>{keyState(key)}</td>
                <td>
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
      {:else}
        <div class="app-problem" data-state="absent"><strong>– No keys yet</strong></div>
      {/if}
      <div class="row">
        {#if page.url.searchParams.has('after')}<a href="?">Newest keys</a>{/if}
        {#if data.keys.next_cursor}<a href="?after={encodeURIComponent(data.keys.next_cursor)}">Older keys</a>{/if}
      </div>
    </section>
  </div>
{/if}

<style>
  .heading {
    margin-bottom: 20px;
  }
  h1 {
    font: var(--weight-strong) 24px var(--font-sans);
  }
  .grid {
    display: grid;
    gap: 20px;
    max-width: 960px;
  }
  section {
    min-width: 0;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
    overflow-x: auto;
  }
  h2 {
    margin: 0 0 12px;
    font: var(--weight-strong) 13px var(--font-sans);
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 0;
    font: 12px var(--font-mono);
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  code {
    font: 12px var(--font-mono);
    overflow-wrap: anywhere;
  }
  p {
    margin: 0;
    font: 12px/18px var(--font-sans);
    color: var(--muted);
  }
  .stack {
    display: grid;
    gap: 8px;
    max-width: 480px;
  }
  label,
  legend {
    font: 700 11px var(--font-mono);
  }
  fieldset {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--border-soft);
    border-radius: var(--r);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 400;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 12px;
    font: 12px var(--font-sans);
  }
  .secret {
    display: grid;
    gap: 8px;
    margin-bottom: 16px;
    padding: 12px;
    border: 1px solid var(--declare);
    border-radius: var(--r);
  }
  table {
    width: 100%;
    margin-top: 16px;
    border-collapse: collapse;
    font: 12px var(--font-sans);
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border-soft);
    text-align: left;
    vertical-align: top;
  }
  th {
    font: 700 11px var(--font-mono);
    color: var(--muted);
  }
</style>
