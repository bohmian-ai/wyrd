<script lang="ts">
  import type { PageProps } from './$types';
  let { data, form }: PageProps = $props();
  let connections = $derived(data.connections);
  let candidate = $derived(connections?.candidate ?? null);
  let tested = $derived(
    !!candidate?.tested_until &&
      candidate.tested_revision === candidate.revision &&
      Date.parse(candidate.tested_until) > Date.now()
  );
  const roles = (map: Record<string, string[]>) =>
    Object.entries(map)
      .map(([group, names]) => `${group} = ${names.join(', ')}`)
      .join('\n');
</script>

<svelte:head><title>Settings · {data.tenant.name} · Wyrd</title></svelte:head>
<div class="heading">
  <h1>Settings</h1>
  <p>Single sign-on for {data.tenant.name}. Secrets are write-only and never shown.</p>
</div>

{#if form?.problem}
  <div class="app-problem" role="alert" data-state={form.problem.status === 403 ? 'unauthorized' : 'error'}>
    <strong>{form.problem.status === 403 ? '⊘' : '✕'} {form.problem.title}</strong>
    <p>{form.problem.remediation}</p>
    <code>{form.problem.code}</code>
  </div>
{/if}

{#if data.problem?.status === 403}
  <div class="app-problem" data-state="unauthorized">
    <strong>⊘ You can't manage sign-in for this tenant</strong>
    <p>Managing the SSO connection requires the identity_connections:write permission.</p>
    <code>{data.problem.code}</code>
  </div>
{:else if data.problem}
  <div class="app-problem" data-state="error">
    <strong>✕ Settings are unavailable</strong>
    <p>{data.problem.remediation}</p>
    <code>{data.problem.code}</code>
  </div>
{:else if connections}
  <div class="grid">
    <section>
      <h2>Callback URL</h2>
      <p>Register this exact redirect URI with your identity provider's Web application.</p>
      <code>{connections.callback_url}</code>
    </section>

    <section>
      <h2>Active connection</h2>
      {#if connections.active}
        {@const active = connections.active}
        <dl>
          <dt>Issuer</dt><dd>{active.issuer}</dd>
          <dt>Client ID</dt><dd>{active.client_id}</dd>
          <dt>Client auth</dt><dd>{active.client_auth}</dd>
          <dt>Revision</dt><dd>{active.revision}</dd>
        </dl>
        <p>Deactivating stops new sign-ins and session renewal through this connection. Access already issued stays valid until it expires (at most five minutes).</p>
        <div class="row">
          <form method="POST" action="?/deactivate">
            <button class="app-control" type="submit">Deactivate</button>
          </form>
          <form method="POST" action="?/remove">
            <input type="hidden" name="id" value={active.id} />
            <button class="app-control" type="submit">Remove</button>
          </form>
        </div>
      {:else}
        <div class="app-problem" data-state="absent">
          <strong>– No active connection</strong>
          <p>Sign-in for this tenant uses API keys until a tested candidate is activated.</p>
        </div>
      {/if}
    </section>

    <section>
      <h2>Candidate connection</h2>
      {#if candidate}
        <dl>
          <dt>Issuer</dt><dd>{candidate.issuer}</dd>
          <dt>Client ID</dt><dd>{candidate.client_id}</dd>
          <dt>Revision</dt><dd>{candidate.revision}</dd>
          <dt>Test</dt><dd class:ok={tested} class:warn={!tested}>{tested ? `✓ Passed · until ${candidate.tested_until}` : '! Not tested'}</dd>
        </dl>
        <div class="row">
          <form method="POST" action="?/test">
            <input type="hidden" name="revision" value={candidate.revision} />
            <button class="app-control" type="submit" title="Sign in at the provider once to test this revision; no session is created">Test sign-in</button>
          </form>
          <form method="POST" action="?/remove">
            <input type="hidden" name="id" value={candidate.id} />
            <button class="app-control" type="submit">Remove</button>
          </form>
        </div>
        {#if tested}
          <form class="stack" method="POST" action="?/activate">
            <input type="hidden" name="revision" value={candidate.revision} />
            <label for="recovery">Recovery API key</label>
            <input id="recovery" class="app-input" type="password" name="recoveryApiKey" required autocomplete="off" />
            <p>A key for a headless principal of this tenant with identity_connections:write. It is verified and discarded.</p>
            <button class="app-control primary" type="submit">Activate</button>
          </form>
        {/if}
      {/if}
      <form class="stack" method="POST" action="?/stage">
        <input type="hidden" name="revision" value={candidate?.revision ?? ''} />
        <label for="issuer">Issuer URL</label>
        <input id="issuer" class="app-input" name="issuer" type="url" required value={candidate?.issuer ?? ''} />
        <label for="client-id">Client ID</label>
        <input id="client-id" class="app-input" name="clientId" required value={candidate?.client_id ?? ''} />
        <label for="client-auth">Client authentication</label>
        <select id="client-auth" class="app-input" name="clientAuth">
          {#each ['SecretBasic', 'SecretPost', 'Public'] as method (method)}
            <option value={method} selected={(candidate?.client_auth ?? 'SecretBasic') === method}>{method}</option>
          {/each}
        </select>
        <label for="client-secret">Client secret</label>
        <input id="client-secret" class="app-input" type="password" name="clientSecret" autocomplete="off" />
        <label for="email-claim">Email claim</label>
        <input id="email-claim" class="app-input" name="emailClaim" value={candidate?.claim_mapping.email ?? 'email'} />
        <label for="groups-claim">Groups claim</label>
        <input id="groups-claim" class="app-input" name="groupsClaim" value={candidate?.claim_mapping.groups ?? ''} />
        <label for="group-roles">Group to role mapping (one <code>group = role, role</code> per line)</label>
        <textarea id="group-roles" class="app-input" name="groupRoles" rows="4">{candidate ? roles(candidate.group_role_map) : ''}</textarea>
        <button class="app-control primary" type="submit">{candidate ? 'Replace candidate' : 'Stage candidate'}</button>
      </form>
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
    max-width: 760px;
  }
  section {
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--r);
    background: var(--surface);
  }
  h2 {
    margin: 0 0 12px;
    font: var(--weight-strong) 13px var(--font-sans);
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 0 0 12px;
    font: 12px var(--font-mono);
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .ok {
    color: var(--ok);
  }
  .warn {
    color: var(--warn);
  }
  code {
    font: 12px var(--font-mono);
    overflow-wrap: anywhere;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 12px;
  }
  .stack {
    display: grid;
    gap: 8px;
    margin-top: 16px;
    padding-top: 16px;
    border-top: 1px dashed var(--border);
  }
  label {
    font: 700 11px var(--font-mono);
  }
  p {
    font: 12px/18px var(--font-sans);
    color: var(--muted);
    margin: 0 0 8px;
  }
</style>
