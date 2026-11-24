<!--
  Settings → Sign-in: the methods people use first, company SSO as a guided
  setup over the server's candidate lifecycle, and Require company SSO as a
  switch whose change is confirmed against the server's own impact count.
  The setup step follows the server's state; only the first two steps, before
  any candidate exists, live in the URL.
-->
<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { fmtInstant } from 'wyrd-ai-ui';
  import { guidance } from '$lib/Entry.svelte';
  import { ICONS } from '$lib/icons';
  import Icon from '$lib/Icon.svelte';
  import Mark from '$lib/Mark.svelte';
  import Overlay from '$lib/Overlay.svelte';
  import Pill from '$lib/Pill.svelte';
  import type { PageProps } from './$types';

  let { data, form }: PageProps = $props();
  const STEPS = ['Register Wyrd with your provider', 'Enter the client details', 'Test sign-in', 'Turn it on'];
  const tenantPath = $derived(`/t/${encodeURIComponent(data.tenant.key)}`);
  const base = $derived(`${tenantPath}/settings/sign-in`);
  const connections = $derived(data.connections);
  const policy = $derived(data.policy);
  const active = $derived(connections?.active ?? null);
  const candidate = $derived(connections?.candidate ?? null);
  const required = $derived(!!policy?.require_company_sso);
  const tested = $derived(
    !!candidate?.tested_until && candidate.tested_revision === candidate.revision && Date.parse(candidate.tested_until) > Date.now()
  );
  const asked = $derived(Number(page.url.searchParams.get('setup') ?? 0));
  /** The server's lifecycle decides the step once a candidate exists; `?setup=2` reopens its details. */
  const step = $derived(candidate ? (asked === 2 ? 2 : tested ? 4 : 3) : asked === 1 || asked === 2 ? asked : 0);
  /** The confirmation opens only for the change the policy can take next. */
  const confirming = $derived(policy?.connection && page.url.searchParams.get('confirm') === (required ? 'allow' : 'require'));
  const locked = $derived(policy?.users_without_company_identity ?? 0);
  const people = (count: number) => `${count} ${count === 1 ? 'person' : 'people'}`;
  const host = (issuer: string) => issuer.replace(/^https?:\/\//, '').replace(/\/.*$/, '');
  const roles = (map: Record<string, string[]>) =>
    Object.entries(map)
      .map(([group, names]) => `${group} = ${names.join(', ')}`)
      .join('\n');
  const TILES = [
    ['google', 'Google', 'Offered by this deployment.'],
    ['github', 'GitHub', 'Offered by this deployment.'],
    ['password', 'Password', 'Username and password.']
  ] as const;
  const offered = (method: 'google' | 'github' | 'password') =>
    method === 'password' ? data.methods?.password : data.methods?.providers.includes(method);
  let copied = $state(false);

  async function copy(text: string) {
    await navigator.clipboard.writeText(text);
    copied = true;
  }
  /** The switch never changes on its own: it asks for confirmation, and the server's answer sets it. */
  function onchange(event: Event & { currentTarget: HTMLInputElement }) {
    event.currentTarget.checked = required;
    goto(`${base}?confirm=${required ? 'allow' : 'require'}`, { noScroll: true });
  }
</script>

<svelte:head><title>Sign-in · {data.tenant.name} · Wyrd</title></svelte:head>
<header class="pagehead">
  <div><h1>Sign-in</h1><p>How people get into {data.tenant.name}.</p></div>
  {#if connections && !active && !step}<div class="actions"><a class="app-control primary lg" href="{base}?setup=1">Set up company SSO</a></div>{/if}
</header>

{#if form?.problem}
  <div class="app-problem" role="alert" data-state={form.problem.status === 403 ? 'unauthorized' : 'error'}>
    <strong>{form.problem.status === 403 ? '⊘' : '✕'} {form.problem.title}</strong>
    {#if form.problem.code === 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED'}
      <p><a href="{tenantPath}/login">Sign in through your company again</a>, then return here and confirm. Nothing was changed.</p>
    {:else if form.problem.status === 409}
      <p>Sign-in settings changed since this page was opened. The page now shows the current settings; review them and try again. Nothing was changed.</p>
    {:else}
      <p>{guidance(form.problem)} Nothing was changed.</p>
    {/if}
    <code>{form.problem.code}</code>
  </div>
{/if}

{#if data.problem?.status === 403}
  <div class="app-problem" data-state="unauthorized">
    <strong>⊘ You can't manage sign-in in this workspace</strong>
    <p>This is for workspace administrators. Ask one to grant what you need.</p>
    <code>{data.problem.code}</code>
  </div>
{:else if data.problem}
  <div class="app-problem" data-state="error">
    <strong>✕ This section is unavailable</strong>
    <p>{data.problem.remediation}</p>
    <code>{data.problem.code}</code>
  </div>
{:else if connections && policy}
  {#if required}
    <div class="note ok" role="status">
      <span class="g" aria-hidden="true">✓</span>
      <div class="grow"><b>Company SSO is required.</b> <span class="muted">Password, Google and GitHub sign-in are off for {data.tenant.name}. Their sessions were ended.</span></div>
    </div>
  {/if}

  <div class="methods">
    {#each TILES as [method, label, detail] (method)}
      {@const on = !required && offered(method)}
      <div class="method-tile">
        <div class="top"><Mark {method} size={18} />{label} <Pill tone={on ? 'ok' : 'off'} label={on ? 'On' : 'Off'} /></div>
        <p>{required ? 'Turned off by Require company SSO.' : on ? detail : 'Not offered by this deployment.'}</p>
      </div>
    {/each}
    <div class="method-tile focus">
      <div class="top">
        <Mark method="sso" size={18} />Company SSO
        {#if active}<Pill tone="ok" label="Active" />{:else if candidate}<Pill tone={tested ? 'info' : 'warn'} label={tested ? 'Tested' : 'Not tested'} />{:else}<Pill tone="off" label="Not set up" />{/if}
      </div>
      <p>{active ? host(active.issuer) : candidate ? `${host(candidate.issuer)} · not on yet` : 'Your identity provider, such as Okta, Entra ID or Google Workspace.'}</p>
    </div>
  </div>

  {#if step}
    <section class="card" aria-label="Set up company SSO">
      <header>
        <h2>Set up company SSO</h2>
        <div class="right">
          {#if candidate}
            <form method="POST" action="?/remove">
              <input type="hidden" name="id" value={candidate.id} />
              <button class="app-control" type="submit">Remove candidate</button>
            </form>
          {:else}
            <a class="app-control" href={base}>Cancel</a>
          {/if}
        </div>
      </header>
      <ol class="steps">
        {#each STEPS as label, index (label)}
          <li class="step" class:done={index + 1 < step} class:now={index + 1 === step} aria-current={index + 1 === step ? 'step' : undefined}>
            <span class="n">{index + 1 < step ? '✓' : index + 1}</span>{label}
          </li>
        {/each}
      </ol>
      {#if step === 1}
        <div class="stepbody">
          <p>In your identity provider, create a <b>Web application</b> and paste this as its redirect URI.</p>
          <div class="field">
            <label for="callback">Redirect URI</label>
            <div class="copyfield">
              <input id="callback" class="app-input mono" readonly value={connections.callback_url ?? ''} />
              <button class="app-control" type="button" onclick={() => copy(connections.callback_url ?? '')}><Icon d={ICONS.copy} size={14} />{copied ? 'Copied' : 'Copy'}</button>
            </div>
          </div>
          <div class="buttons"><a class="app-control primary" href="{base}?setup=2">I've registered it</a></div>
        </div>
      {:else if step === 2}
        <form class="stepbody" method="POST" action="?/stage">
          <input type="hidden" name="revision" value={candidate?.revision ?? ''} />
          <div class="field">
            <label for="issuer">Issuer URL</label>
            <input id="issuer" class="app-input mono" name="issuer" type="url" required defaultValue={candidate?.issuer ?? ''} />
            <span class="hint">Wyrd reads the provider's settings from this address.</span>
          </div>
          <div class="field">
            <label for="client-id">Client ID</label>
            <input id="client-id" class="app-input mono" name="clientId" required defaultValue={candidate?.client_id ?? ''} />
          </div>
          <div class="field">
            <label for="client-secret">Client secret</label>
            <input id="client-secret" class="app-input mono" type="password" name="clientSecret" autocomplete="off" />
            <span class="hint">Stored encrypted. Wyrd never shows it again.</span>
          </div>
          <details class="advanced">
            <summary>Advanced</summary>
            <div class="field">
              <label for="client-auth">Client authentication</label>
              <select id="client-auth" class="app-input" name="clientAuth">
                {#each ['SecretBasic', 'SecretPost', 'Public'] as method (method)}
                  <option value={method} selected={(candidate?.client_auth ?? 'SecretBasic') === method}>{method}</option>
                {/each}
              </select>
            </div>
            <div class="field">
              <label for="email-claim">Email claim</label>
              <input id="email-claim" class="app-input mono" name="emailClaim" defaultValue={candidate?.claim_mapping.email ?? 'email'} />
            </div>
            <div class="field">
              <label for="groups-claim">Groups claim</label>
              <input id="groups-claim" class="app-input mono" name="groupsClaim" defaultValue={candidate?.claim_mapping.groups ?? ''} />
            </div>
            <div class="field">
              <label for="group-roles">Group to role mapping</label>
              <textarea id="group-roles" class="app-input mono" name="groupRoles" rows="4">{candidate ? roles(candidate.group_role_map) : ''}</textarea>
              <span class="hint">One <code>group = role, role</code> per line.</span>
            </div>
          </details>
          <div class="buttons">
            <a class="app-control" href={candidate ? base : `${base}?setup=1`}>Back</a>
            <button class="app-control primary" type="submit">Save client details</button>
          </div>
        </form>
      {:else if candidate}
        <div class="stepbody">
          <dl class="facts">
            <dt>Issuer</dt><dd class="mono">{candidate.issuer}</dd>
            <dt>Client ID</dt><dd class="mono">{candidate.client_id}</dd>
            <dt>Test</dt>
            <dd>{#if tested}<Pill tone="ok" label="Passed" /> <span class="muted">until {fmtInstant(Date.parse(candidate.tested_until ?? ''))}</span>{:else}<Pill tone="warn" label="Not tested" />{/if}</dd>
          </dl>
          {#if step === 3}
            <p>Sign in once at your provider to test these details. No session is created.</p>
            <div class="buttons">
              <a class="app-control" href="{base}?setup=2">Edit details</a>
              <form method="POST" action="?/test">
                <input type="hidden" name="revision" value={candidate.revision} />
                <button class="app-control primary" type="submit">Test sign-in</button>
              </form>
            </div>
          {:else}
            <form class="activate" method="POST" action="?/activate">
              <input type="hidden" name="revision" value={candidate.revision} />
              <div class="field">
                <label for="recovery">Recovery API key</label>
                <input id="recovery" class="app-input mono" type="password" name="recoveryApiKey" required autocomplete="off" />
                <span class="hint">A key for a service account of {data.tenant.name} that may manage sign-in. Wyrd checks it and discards it.</span>
              </div>
              <div class="buttons">
                <a class="app-control" href="{base}?setup=2">Edit details</a>
                <button class="app-control primary" type="submit">Turn on company SSO</button>
              </div>
            </form>
          {/if}
        </div>
      {/if}
    </section>
  {/if}

  <section class="card">
    <header><h2>Require company SSO</h2></header>
    <div class="row">
      <div class="label">
        <b>Only allow sign-in through your company</b>
        <span>{active ? `Turns off password, Google and GitHub sign-in for ${data.tenant.name} and ends those sessions.` : 'Available once company SSO is active.'}</span>
      </div>
      <div class="ctl">
        <label class="switch">
          <input type="checkbox" role="switch" checked={required} disabled={!policy.connection || (!policy.ready && !required)} {onchange} />
          {required ? 'On' : 'Off'}
        </label>
      </div>
    </div>
    {#if active && !required}
      <div class="row">
        <div class="note warn" role="status">
          <span class="g" aria-hidden="true">!</span>
          <div class="grow">{people(locked)} {locked === 1 ? 'has' : 'have'} no company sign-in yet and would be locked out. <a href="{tenantPath}/settings/people">See who</a></div>
        </div>
      </div>
    {/if}
  </section>

  {#if active}
    <section class="danger-zone">
      <header><h2>Danger zone</h2></header>
      <div class="row">
        <div class="label"><b>Deactivate company SSO</b><span>People who sign in through {host(active.issuer)} can no longer get in, and their sessions end. Turn off Require company SSO first.</span></div>
        <div class="ctl">
          <form method="POST" action="?/deactivate"><button class="app-control danger" type="submit" disabled={required}>Deactivate</button></form>
        </div>
      </div>
      <div class="row">
        <div class="label"><b>Remove company SSO</b><span>Deletes this connection. Setting up company SSO again starts from the first step.</span></div>
        <div class="ctl">
          <form method="POST" action="?/remove">
            <input type="hidden" name="id" value={active.id} />
            <button class="app-control danger" type="submit">Remove</button>
          </form>
        </div>
      </div>
    </section>
  {/if}

  {#if confirming && policy.connection}
    <Overlay kind="dialog" title={required ? 'Allow other sign-in methods?' : 'Require company SSO?'} close={base}>
      <form id="policy" method="POST" action="?/policy">
        <input type="hidden" name="require" value={String(!required)} />
        <input type="hidden" name="revision" value={policy.revision} />
        <input type="hidden" name="connectionId" value={policy.connection.id} />
        <input type="hidden" name="connectionRevision" value={policy.connection.revision} />
        <p>This takes effect immediately:</p>
        {#if required}
          <ul class="impact"><li>Password, Google and GitHub sign-in work again for {data.tenant.name}, wherever this deployment offers them.</li></ul>
        {:else}
          <ul class="impact">
            <li>Password, Google and GitHub sign-in stop for {data.tenant.name}.</li>
            <li><b>{people(locked)}</b> without a company account {locked === 1 ? 'is' : 'are'} locked out until they sign in through your company. Matching email does not link accounts.</li>
          </ul>
        {/if}
        <p class="muted small">Confirms that you signed in with your company in the last 5 minutes.</p>
        <label class="check"><input type="checkbox" name="confirm" required /> {required ? 'I understand other sign-in methods turn back on' : `I understand ${people(locked)} will lose access`}</label>
      </form>
      {#snippet footer()}
        <a class="app-control" href={base}>Cancel</a>
        <button class="app-control {required ? 'primary' : 'danger fill'}" type="submit" form="policy">{required ? 'Allow other methods' : 'Require company SSO'}</button>
      {/snippet}
    </Overlay>
  {/if}
{/if}

<style>
  .card > header form,
  .buttons form,
  .ctl form {
    display: contents;
  }
  .steps {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .advanced {
    display: grid;
    gap: 14px;
  }
  .advanced summary {
    cursor: pointer;
    font-size: 13px;
    color: var(--muted);
  }
  .advanced[open] summary {
    margin-bottom: 14px;
  }
  .advanced .field + .field {
    margin-top: 14px;
  }
  .activate {
    display: grid;
    gap: 14px;
  }
  .facts {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 8px 16px;
    margin: 0;
    font-size: 13px;
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .row .note {
    flex: 1;
    margin: 0;
  }
  .impact {
    display: grid;
    gap: 6px;
    padding-left: 18px;
    list-style: disc;
  }
  .small {
    font-size: 13px;
  }
</style>
